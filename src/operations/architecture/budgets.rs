use super::contract::component_for;
use super::rules::stable_hash;
use super::source_metrics;
use crate::engine::RepositoryState;
use blazingly_json::{Value, json};
use std::collections::BTreeMap;

const BUDGET_KEYS: &[&str] = &["maxFileLoc", "maxFunctionLoc", "runtimeCycles"];

pub(super) fn validate(value: &Value) -> Result<(), String> {
    let Some(budgets) = value.get("budgets") else {
        return Ok(());
    };
    let Some(budgets) = budgets.as_object() else {
        return Err("architecture contract budgets must be an object".to_owned());
    };
    for key in BUDGET_KEYS {
        if let Some(limit) = budgets.get(*key)
            && limit.as_u64().is_none()
        {
            return Err(format!(
                "architecture budget {key} must be a non-negative integer"
            ));
        }
    }
    Ok(())
}

pub(super) fn violations(state: &RepositoryState, value: &Value) -> Result<Vec<Value>, String> {
    let max_file = budget(value, "maxFileLoc");
    let max_function = budget(value, "maxFunctionLoc");
    let max_cycles = budget(value, "runtimeCycles");
    let mut output = BTreeMap::<String, Value>::new();

    if max_file.is_some() || max_function.is_some() {
        let metrics = source_metrics::collect(state, value, max_function.is_some())?;
        if let Some(maximum) = max_file {
            for file in metrics.files {
                if file.lines > maximum {
                    let identity = format!("budget.maxFileLoc|{}", file.path);
                    insert(
                        &mut output,
                        &identity,
                        "budget.maxFileLoc",
                        maximum,
                        &json!({
                            "kind": "file_loc",
                            "file": file.path,
                            "actual": file.lines,
                            "maximum": maximum
                        }),
                    );
                }
            }
        }
        if let Some(maximum) = max_function {
            for function in metrics.functions {
                if function.lines > maximum {
                    let identity = format!(
                        "budget.maxFunctionLoc|{}|{}|{}|{}",
                        function.file, function.kind, function.name, function.start_line
                    );
                    insert(
                        &mut output,
                        &identity,
                        "budget.maxFunctionLoc",
                        maximum,
                        &json!({
                            "kind": "function_loc",
                            "file": function.file,
                            "symbol": function.name,
                            "symbol_kind": function.kind,
                            "start_line": function.start_line,
                            "actual": function.lines,
                            "maximum": maximum
                        }),
                    );
                }
            }
        }
    }

    if let Some(maximum) = max_cycles {
        let cycles = super::super::health::runtime_dependency_cycles(state.graph(), &json!({}))
            .into_iter()
            .filter(|cycle| {
                cycle.iter().any(|member| {
                    member
                        .strip_prefix("file:")
                        .is_some_and(|path| component_for(value, path).is_some())
                })
            })
            .collect::<Vec<_>>();
        let actual = u64::try_from(cycles.len()).unwrap_or(u64::MAX);
        if actual > maximum {
            insert(
                &mut output,
                "budget.runtimeCycles",
                "budget.runtimeCycles",
                maximum,
                &json!({
                    "kind": "runtime_cycles",
                    "actual": actual,
                    "maximum": maximum,
                    "cycles": cycles
                }),
            );
        }
    }

    Ok(output.into_values().collect())
}

fn budget(value: &Value, key: &str) -> Option<u64> {
    value
        .pointer(&format!("/budgets/{key}"))
        .and_then(Value::as_u64)
}

fn insert(
    output: &mut BTreeMap<String, Value>,
    identity: &str,
    rule_id: &str,
    maximum: u64,
    evidence: &Value,
) {
    let fingerprint = stable_hash(identity);
    output.insert(
        fingerprint.clone(),
        json!({
            "fingerprint": fingerprint,
            "category": "budget",
            "rule": {
                "id": rule_id,
                "action": "limit",
                "maximum": maximum
            },
            "evidence": evidence
        }),
    );
}
