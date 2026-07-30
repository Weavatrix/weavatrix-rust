use crate::engine::RepositoryState;
use blazingly_json::{Value, json};
use std::collections::BTreeSet;
use std::fs;
use std::io::ErrorKind;

pub(super) fn load_optional(state: &RepositoryState) -> Result<Option<Value>, String> {
    let path = state.root().join(".weavatrix/architecture.json");
    let bytes = match fs::read(&path) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(format!("{}: {error}", path.display())),
    };
    let value: Value =
        blazingly_json::from_slice(&bytes).map_err(|error| format!("invalid contract: {error}"))?;
    if value.get("components").and_then(Value::as_array).is_none() {
        return Err("architecture contract has no components".to_owned());
    }
    Ok(Some(value))
}

pub(super) fn remediation() -> Value {
    json!({
        "offline_path": ".weavatrix/architecture.json",
        "next_tool": "verify_architecture"
    })
}

pub(super) fn not_configured(state: &RepositoryState, reason: &str) -> Value {
    json!({
        "state": "NOT_CONFIGURED",
        "reason": reason,
        "starter": starter(state),
        "remediation": remediation(),
        "write": "NONE"
    })
}

pub(super) fn starter(state: &RepositoryState) -> Value {
    let mut roots = BTreeSet::new();
    for node in state
        .graph()
        .nodes()
        .iter()
        .filter(|node| node.kind == weavatrix_graph::NodeKind::File)
    {
        roots.insert(node.label.split('/').next().unwrap_or("root").to_owned());
    }
    json!({
        "architectureContractV": 1,
        "name": "Derived no-regressions architecture",
        "style": "modular-components",
        "enforcement": "ratchet",
        "components": roots.into_iter().map(|root| {
            json!({"id": root.replace('_', "-"), "name": root, "paths": [root]})
        }).collect::<Vec<_>>(),
        "dependencyRules": [],
        "budgets": {
            "runtimeCycles": 0,
            "maxFileLoc": 300,
            "maxFunctionLoc": 100
        },
        "exceptions": [],
        "ratchet": {"baseline": {"fingerprints": [], "metrics": {}}}
    })
}

pub(super) fn rules_for_file<'contract>(
    value: &'contract Value,
    file: &str,
) -> Vec<&'contract Value> {
    let Some(component) = component_for(value, file) else {
        return Vec::new();
    };
    value
        .get("dependencyRules")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter(|rule| {
            list_contains(rule.get("from"), component) || list_contains(rule.get("to"), component)
        })
        .collect()
}

pub(super) fn component_for<'contract>(
    value: &'contract Value,
    file: &str,
) -> Option<&'contract str> {
    value
        .get("components")?
        .as_array()?
        .iter()
        .filter_map(|component| {
            let id = component.get("id")?.as_str()?;
            let longest = component
                .get("paths")?
                .as_array()?
                .iter()
                .filter_map(Value::as_str)
                .filter(|path| file == *path || file.starts_with(&format!("{path}/")))
                .map(str::len)
                .max()?;
            Some((longest, id))
        })
        .max_by_key(|(length, _)| *length)
        .map(|(_, id)| id)
}

pub(super) fn list_contains(value: Option<&Value>, expected: &str) -> bool {
    value
        .and_then(Value::as_array)
        .is_some_and(|values| values.iter().any(|value| value.as_str() == Some(expected)))
}
