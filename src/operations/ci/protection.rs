use crate::engine::RepositoryState;
use blazingly_json::{Value, json};

pub(super) fn attach(state: &RepositoryState, name: &str, args: &Value, report: &mut Value) {
    if !matches!(
        name,
        "prepare_change" | "change_impact" | "verified_change" | "graph_diff"
    ) {
        return;
    }
    let files = if name == "prepare_change" {
        args.get("files")
    } else if name == "verified_change" {
        report.pointer("/impact/changed_files")
    } else {
        report.get("changed_files")
    };
    let changed_total = files.and_then(Value::as_array).map_or(0, Vec::len);
    let changed = files
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .take(100)
        .map(str::to_owned)
        .collect::<Vec<_>>();
    let Some(object) = report.as_object_mut() else {
        return;
    };
    let memberships = crate::operations::architecture::structural_memberships(state);
    let root_component = crate::operations::architecture::root_structural_id(state);
    let build = crate::operations::build::model(state);
    let declaration = crate::operations::architecture::contract::load_optional(state);
    let selected = selected_files(
        &changed,
        &memberships,
        &root_component,
        declaration.as_ref().ok().and_then(Option::as_ref),
        &build,
    );
    let restrictions = super::restrictions::report(state, &json!({"max_results": 100}));
    let restrictions_error = restrictions.as_ref().err().cloned();
    let checks = restrictions
        .as_ref()
        .ok()
        .and_then(|report| report.get("restrictions"))
        .cloned()
        .unwrap_or_else(|| json!([]));
    let checks_total = restrictions
        .as_ref()
        .ok()
        .and_then(|report| report["restrictions_total"].as_u64());
    let candidates = checks
        .as_array()
        .into_iter()
        .flatten()
        .map(|check| check_candidate(check, &selected, &build))
        .collect::<Vec<_>>();
    let (rule_bindings, bindings_total) = declaration
        .as_ref()
        .ok()
        .and_then(Option::as_ref)
        .map_or((Vec::new(), 0), |contract| {
            declared_rule_bindings(state, contract, &selected, &checks)
        });
    let mut diagnostics = [restrictions_error, declaration.err()]
        .into_iter()
        .flatten()
        .collect::<Vec<_>>();
    if let Ok(ref child) = restrictions
        && child["status"] == "INCOMPLETE"
    {
        diagnostics.push("ci_restrictions has unresolved evidence".to_owned());
    }
    let truncated = changed_total > changed.len()
        || checks_total.is_some_and(|total| total > candidates.len() as u64)
        || bindings_total > rule_bindings.len();
    object.insert(
        "ci_protection".to_owned(),
        json!({
            "status": if diagnostics.is_empty() && !truncated {"STATIC_CANDIDATES"} else {"INCOMPLETE"},
            "diagnostics": diagnostics,
            "truncated": truncated,
            "changed_total": changed_total,
            "candidate_checks_total": checks_total,
            "restrictions_status": restrictions.as_ref().ok().map(|child| &child["status"]),
            "restrictions_unresolved": restrictions.as_ref().ok().map(|child| &child["unresolved"]),
            "declared_rule_bindings_total": bindings_total,
            "changed": selected,
            "candidate_checks": candidates,
            "declared_rule_bindings": rule_bindings,
            "reason": "local workflow checks are candidates; event, path filters, execution and remote enforcement are not proven"
        }),
    );
}

fn selected_files(
    changed: &[String],
    memberships: &[(String, String)],
    root_component: &str,
    declaration: Option<&Value>,
    build: &crate::operations::build::BuildModel,
) -> Vec<Value> {
    changed
        .iter()
        .map(|file| {
            let mut declared = declaration
                .map(|value| crate::operations::architecture::contract::components_for(value, file))
                .unwrap_or_default()
                .into_iter()
                .map(str::to_owned)
                .collect::<Vec<_>>();
            declared.sort();
            declared.dedup();
            let unique = (declared.len() == 1).then(|| declared[0].clone());
            let component = memberships
                .iter()
                .filter(|(path, _)| {
                    if path.is_empty() {
                        !file.contains('/')
                    } else {
                        file.starts_with(&format!("{path}/")) || file == path
                    }
                })
                .max_by_key(|(path, _)| path.len());
            let build_membership = build.file_membership(file);
            json!({
                "file": file,
                "component": component.map_or_else(|| if file.contains('/') { Value::Null }
                        else { json!(root_component) }, |(_, id)| json!(id)),
                "declared_component": unique,
                "declared_components": declared,
                "build_membership": build_membership
            })
        })
        .collect()
}

fn check_candidate(
    check: &Value,
    selected: &[Value],
    build: &crate::operations::build::BuildModel,
) -> Value {
    let mut members = selected
        .iter()
        .flat_map(|file| {
            file["build_membership"]["member_ids"]
                .as_array()
                .into_iter()
                .flatten()
        })
        .filter_map(Value::as_str)
        .map(str::to_owned)
        .collect::<Vec<_>>();
    members.sort();
    members.dedup();
    let kind = check["kind"].as_str().unwrap_or_default();
    let target = check["target"].as_str();
    let (targets, tasks) = match kind {
        "cargo_test" => (build.target_ids("test", target, &members), Vec::new()),
        "npm_script" => (
            Vec::new(),
            target.map_or_else(Vec::new, |name| build.task_ids(name, &members)),
        ),
        _ => (Vec::new(), Vec::new()),
    };
    let relation = if !targets.is_empty() {
        "TARGET_CHECK_CANDIDATE"
    } else if !tasks.is_empty() {
        "TASK_CHECK_CANDIDATE"
    } else if !members.is_empty() && target.is_none() {
        "MEMBER_CHECK_CANDIDATE"
    } else {
        "PROJECT_WORKFLOW_CANDIDATE"
    };
    json!({
        "id": check["id"],
        "kind": check["kind"],
        "relation": relation,
        "selected_members": members,
        "selected_targets": targets,
        "selected_tasks": tasks,
        "selection_basis": "STATIC_BUILD_MODEL",
        "failure_effect": check["failure_effect"],
        "recognition": check["recognition"]
    })
}

fn declared_rule_bindings(
    state: &RepositoryState,
    contract: &Value,
    selected: &[Value],
    checks: &Value,
) -> (Vec<Value>, usize) {
    let Some(rules) = contract.get("dependencyRules").and_then(Value::as_array) else {
        return (Vec::new(), 0);
    };
    let verifier_reference = architecture_test_references_verifier(state);
    let checker = checks.as_array().into_iter().flatten().find(|check| {
        check["kind"] == "cargo_test"
            && check["target"] == "architecture"
            && check["target_resolved"] == true
    });
    let matching = rules
        .iter()
        .filter(|rule| {
            rule["from"].as_array().is_some_and(|sources| {
                sources.iter().any(|source| {
                    selected.iter().any(|file| {
                        file["declared_components"]
                            .as_array()
                            .is_some_and(|ids| ids.contains(source))
                    })
                })
            })
        })
        .collect::<Vec<_>>();
    let total = matching.len();
    (
        matching
            .into_iter()
            .take(50)
            .map(|rule| {
                json!({
                    "rule_id": rule["id"],
                    "declaration": ".weavatrix/architecture.json",
                    "checker": checker.map(|check| &check["id"]),
                    "linkage": if checker.is_some() && verifier_reference {
                        "STATIC_REFERENCE_CANDIDATE"
                    } else {
                        "UNBOUND"
                    },
                    "execution": "NOT_OBSERVED",
                    "merge_requirement": "NOT_OBSERVED"
                })
            })
            .collect(),
        total,
    )
}

fn architecture_test_references_verifier(state: &RepositoryState) -> bool {
    state
        .evidence()
        .paths()
        .filter(|path| {
            path.starts_with("tests/architecture/")
                && std::path::Path::new(path)
                    .extension()
                    .is_some_and(|extension| extension.eq_ignore_ascii_case("rs"))
        })
        .take(100)
        .any(|path| {
            state
                .evidence()
                .text(path)
                .is_some_and(|text| text.contains("verify_architecture"))
        })
}
