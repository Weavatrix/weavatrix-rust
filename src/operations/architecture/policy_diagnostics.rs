use super::contract::{component_for, list_contains};
use super::rules::stable_hash;
use crate::engine::RepositoryState;
use blazingly_json::{Value, json};
use std::collections::BTreeMap;

pub(super) fn violations(state: &RepositoryState, contract: &Value) -> Vec<Value> {
    let mut output = BTreeMap::new();
    for diagnostic in state
        .snapshot()
        .diagnostics
        .iter()
        .filter(|diagnostic| diagnostic.code == "import.unresolved")
    {
        let Some(span) = diagnostic.span.as_ref() else {
            continue;
        };
        let Some(component) = component_for(contract, &span.file) else {
            continue;
        };
        for rule in unresolved_rules(contract, component) {
            let identity = format!(
                "{}|unresolved|{}|{}",
                rule["id"].as_str().unwrap_or("rule"),
                span.file,
                diagnostic.message
            );
            let fingerprint = stable_hash(&identity);
            output.entry(fingerprint.clone()).or_insert_with(|| {
                json!({
                    "fingerprint": fingerprint,
                    "category": "dependency",
                    "rule": rule,
                    "source": {
                        "file": span.file,
                        "component": component
                    },
                    "evidence": {
                        "kind": "unresolved_dependency",
                        "diagnostic": diagnostic
                    }
                })
            });
        }
    }
    output.into_values().collect()
}

fn unresolved_rules<'contract>(
    contract: &'contract Value,
    component: &str,
) -> impl Iterator<Item = &'contract Value> {
    contract
        .get("dependencyRules")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter(|rule| rule["action"] == "forbid")
        .filter(|rule| list_contains(rule.get("from"), component))
        .filter(|rule| list_contains(rule.get("kinds"), "unresolved"))
}
