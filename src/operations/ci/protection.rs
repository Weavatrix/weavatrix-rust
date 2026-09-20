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
    let inventory = crate::operations::architecture::inventory(state);
    let components = inventory["components"].as_array();
    let selected = changed
        .iter()
        .map(|file| {
            let component = components
                .into_iter()
                .flatten()
                .filter_map(|item| Some((item, item["path"].as_str()?)))
                .find(|(_, path)| file.starts_with(&format!("{path}/")) || file == path);
            json!({
                "file": file,
                "component": component.map(|(item, _)| &item["id"]),
                "declared_component": component.map(|(item, _)| &item["declared_id"])
            })
        })
        .collect::<Vec<_>>();
    let checks = super::restrictions::report(state, &json!({"max_results": 100}))
        .ok()
        .and_then(|report| report.get("restrictions").cloned())
        .unwrap_or_else(|| json!([]));
    let candidates = checks
        .as_array()
        .into_iter()
        .flatten()
        .map(|check| {
            json!({
                "id": check["id"],
                "kind": check["kind"],
                "relation": "PROJECT_WORKFLOW_CANDIDATE",
                "failure_effect": check["failure_effect"],
                "recognition": check["recognition"]
            })
        })
        .collect::<Vec<_>>();
    let rule_bindings = declared_rule_bindings(state, &selected, &checks);
    object.insert(
        "ci_protection".to_owned(),
        json!({
            "status": "STATIC_CANDIDATES",
            "changed": selected,
            "candidate_checks": candidates,
            "declared_rule_bindings": rule_bindings,
            "reason": "local workflow checks are candidates; event, path filters, execution and remote enforcement are not proven"
        }),
    );
}

fn declared_rule_bindings(
    state: &RepositoryState,
    selected: &[Value],
    checks: &Value,
) -> Vec<Value> {
    let Ok(contract) = crate::operations::architecture::contract(state, &json!({})) else {
        return Vec::new();
    };
    let Some(rules) = contract
        .pointer("/contract/dependencyRules")
        .and_then(Value::as_array)
    else {
        return Vec::new();
    };
    let verifier_reference = architecture_test_references_verifier(state.root());
    let checker = checks.as_array().into_iter().flatten().find(|check| {
        check["kind"] == "cargo_test"
            && check["target"] == "architecture"
            && check["target_resolved"] == true
    });
    rules
        .iter()
        .filter(|rule| {
            rule["from"].as_array().is_some_and(|sources| {
                sources.iter().any(|source| {
                    selected
                        .iter()
                        .any(|file| file["declared_component"] == *source)
                })
            })
        })
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
        .collect()
}

fn architecture_test_references_verifier(root: &std::path::Path) -> bool {
    let directory = root.join("tests/architecture");
    let Ok(entries) = std::fs::read_dir(directory) else {
        return false;
    };
    entries.flatten().take(100).any(|entry| {
        std::fs::read_to_string(entry.path())
            .ok()
            .is_some_and(|text| text.contains("verify_architecture"))
    })
}
