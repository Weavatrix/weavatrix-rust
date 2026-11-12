use crate::engine::RepositoryState;
use blazingly_json::{Value, json};

pub(super) fn report(state: &RepositoryState, args: &Value) -> Result<Value, String> {
    crate::operations::reject_unknown_arguments("ci_protection", args, &["files"])?;
    let files = args
        .get("files")
        .ok_or_else(|| "files is required".to_owned())?;
    let files = files
        .as_array()
        .ok_or_else(|| "files must be an array of strings".to_owned())?;
    let changed_all = files
        .iter()
        .map(|item| {
            item.as_str()
                .map(str::to_owned)
                .ok_or_else(|| "files must contain only strings".to_owned())
        })
        .collect::<Result<Vec<_>, String>>()?;
    let changed_total = changed_all.len();
    let changed = changed_all.into_iter().take(100).collect::<Vec<_>>();
    let memberships = crate::operations::architecture::structural_memberships(state);
    let build = crate::operations::build::model(state);
    let declaration = crate::operations::architecture::contract::load_optional(state);
    let mut selected = selected_files(
        &changed,
        &memberships,
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
            super::linkage::declared_rule_bindings(state, &build, contract, &selected, &checks)
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
    let (unresolved, unresolved_total) = unresolved_page(restrictions.as_ref().ok());
    let unresolved_len = unresolved.as_array().map_or(0, Vec::len);
    let truncated = changed_total > changed.len()
        || checks_total.is_some_and(|total| total > candidates.len() as u64)
        || bindings_total > rule_bindings.len()
        || unresolved_total.is_some_and(|total| total > unresolved_len);
    present_membership(&build, &mut selected);
    Ok(json!({
        "status": if diagnostics.is_empty() && !truncated {"STATIC_CANDIDATES"} else {"INCOMPLETE"},
        "diagnostics": diagnostics,
        "truncated": truncated,
        "changed_total": changed_total,
        "candidate_checks_total": checks_total,
        "restrictions_status": restrictions.as_ref().ok().map(|child| &child["status"]),
        "restrictions_unresolved": unresolved,
        "restrictions_unresolved_total": unresolved_total,
        "declared_rule_bindings_total": bindings_total,
        "changed": selected,
        "candidate_checks": candidates,
        "declared_rule_bindings": rule_bindings,
        "reason": "local workflow checks are candidates; event, path filters, execution and remote enforcement are not proven"
    }))
}

fn unresolved_page(restrictions: Option<&Value>) -> (Value, Option<usize>) {
    let Some(report) = restrictions else {
        return (Value::Null, None);
    };
    let Some(items) = report.get("unresolved").and_then(Value::as_array) else {
        return (json!([]), Some(0));
    };
    (
        json!(items.iter().take(20).cloned().collect::<Vec<_>>()),
        Some(items.len()),
    )
}

fn selected_files(
    changed: &[String],
    memberships: &[(String, String)],
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
            let component = match component {
                Some((path, _)) if path.is_empty() => json!("component:root-files:"),
                Some((path, _)) => json!(format!("component:directory:{path}")),
                None if !file.contains('/') => json!("component:root-files:"),
                None => Value::Null,
            };
            // Full build ids stay here until candidate selection has used them.
            json!({
                "file": file,
                "component": component,
                "declared_component": unique,
                "declared_components": declared,
                "build_membership": build.file_membership(file)
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
        "selected_members": members.iter().map(|id| build.member_label(id)).collect::<Vec<_>>(),
        "selected_targets": targets.iter().map(|id| build.target_label(id)).collect::<Vec<_>>(),
        "selected_tasks": tasks.iter().map(|id| build.task_label(id)).collect::<Vec<_>>(),
        "selection_basis": "STATIC_BUILD_MODEL",
        "failure_effect": check["failure_effect"],
        "recognition": check["recognition"]
    })
}

fn present_membership(build: &crate::operations::build::BuildModel, selected: &mut [Value]) {
    for item in selected {
        let Some(membership) = item.get("build_membership").cloned() else {
            continue;
        };
        let labeled = json!({
            "resolution": membership.get("resolution"),
            "member_ids": labeled_ids(&membership, "member_ids", |id| build.member_label(id)),
            "target_ids": labeled_ids(&membership, "target_ids", |id| build.target_label(id)),
            "module_ids": labeled_ids(&membership, "module_ids", |id| build.module_label(id))
        });
        if let Some(object) = item.as_object_mut() {
            object.insert("build_membership".to_owned(), labeled);
        }
    }
}

fn labeled_ids(membership: &Value, key: &str, label: impl Fn(&str) -> String) -> Vec<String> {
    membership
        .get(key)
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .map(label)
        .collect()
}
