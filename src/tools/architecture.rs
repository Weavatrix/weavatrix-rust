use crate::RepositoryState;
use crate::tools::arg_str;
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;

pub fn contract(state: &RepositoryState, args: &Value) -> Result<Value, String> {
    if arg_str(args, "action").ok() == Some("approve") {
        return Err("read-only MCP never writes architecture contracts".to_owned());
    }
    match load(state) {
        Ok(contract) => Ok(json!({
            "state": "CONFIGURED",
            "source": ".weavatrix/architecture.json",
            "contract": contract
        })),
        Err(reason) if arg_str(args, "action").ok() == Some("preview") => Ok(json!({
            "state": "PREVIEW",
            "source": "derived graph folders",
            "contract": starter(state),
            "warning": reason,
            "write": "NONE"
        })),
        Err(reason) => Ok(json!({"state": "NOT_CONFIGURED", "reason": reason})),
    }
}

pub fn prepare(state: &RepositoryState, args: &Value) -> Result<Value, String> {
    let contract = load(state)?;
    let files = args
        .get("files")
        .and_then(Value::as_array)
        .ok_or_else(|| "files must be an array".to_owned())?;
    let selected = files
        .iter()
        .filter_map(Value::as_str)
        .map(|file| {
            json!({
                "file": file,
                "component": component_for(&contract, file),
                "rules": rules_for_file(&contract, file)
            })
        })
        .collect::<Vec<_>>();
    Ok(json!({
        "intent": args.get("intent"),
        "files": selected,
        "contract": ".weavatrix/architecture.json"
    }))
}

pub fn verify(state: &RepositoryState) -> Result<Value, String> {
    let contract = load(state)?;
    let baseline = contract
        .pointer("/ratchet/baseline/fingerprints")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .collect::<BTreeSet<_>>();
    let violations = violations(state, &contract);
    let (existing, new): (Vec<_>, Vec<_>) = violations
        .into_iter()
        .partition(|item| baseline.contains(item["fingerprint"].as_str().unwrap_or_default()));
    Ok(json!({
        "state": if new.is_empty() {"PASS"} else {"BLOCKED"},
        "new": new,
        "existing": existing,
        "fixed": [],
        "contract": ".weavatrix/architecture.json"
    }))
}

pub fn explain(state: &RepositoryState, args: &Value) -> Result<Value, String> {
    let fingerprint = arg_str(args, "fingerprint")?;
    let contract = load(state)?;
    let violation = violations(state, &contract)
        .into_iter()
        .find(|item| item["fingerprint"] == fingerprint)
        .ok_or_else(|| format!("active architecture violation not found: {fingerprint}"))?;
    Ok(json!({"violation": violation, "contract": ".weavatrix/architecture.json"}))
}

pub fn propose_exception(state: &RepositoryState, args: &Value) -> Result<Value, String> {
    let fingerprint = arg_str(args, "fingerprint")?;
    let reason = arg_str(args, "reason")?;
    let contract = load(state)?;
    let violation = violations(state, &contract)
        .into_iter()
        .find(|item| item["fingerprint"] == fingerprint)
        .ok_or_else(|| format!("active architecture violation not found: {fingerprint}"))?;
    Ok(json!({
        "state": "PROPOSAL_ONLY",
        "proposal": {
            "fingerprint": fingerprint,
            "reason": reason,
            "expires": args.get("expires"),
            "violation": violation
        },
        "write": "NONE"
    }))
}

fn load(state: &RepositoryState) -> Result<Value, String> {
    let path = state.root().join(".weavatrix/architecture.json");
    let bytes = fs::read(&path).map_err(|error| format!("{}: {error}", path.display()))?;
    let value: Value =
        serde_json::from_slice(&bytes).map_err(|error| format!("invalid contract: {error}"))?;
    if value.get("components").and_then(Value::as_array).is_none() {
        return Err("architecture contract has no components".to_owned());
    }
    Ok(value)
}

fn starter(state: &RepositoryState) -> Value {
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
        "budgets": {"runtimeCycles": 0, "maxFileLoc": 300},
        "exceptions": [],
        "ratchet": {"baseline": {"fingerprints": [], "metrics": {}}}
    })
}

fn violations(state: &RepositoryState, contract: &Value) -> Vec<Value> {
    let mut output = BTreeMap::<String, Value>::new();
    for edge in state.graph().edges() {
        let Some(source) = state.graph().node(edge.source.as_str()) else {
            continue;
        };
        let Some(target) = state.graph().node(edge.target.as_str()) else {
            continue;
        };
        let Some(source_file) = node_file(source) else {
            continue;
        };
        let Some(target_file) = node_file(target) else {
            continue;
        };
        let Some(from) = component_for(contract, source_file) else {
            continue;
        };
        let Some(to) = component_for(contract, target_file) else {
            continue;
        };
        if from == to {
            continue;
        }
        for rule in matching_rules(contract, from, to, edge.kind.as_str()) {
            let plain = format!(
                "{}|{}|{}|{}",
                rule["id"].as_str().unwrap_or("rule"),
                edge.source,
                edge.target,
                edge.kind.as_str()
            );
            let fingerprint = stable_hash(&plain);
            output.entry(fingerprint.clone()).or_insert_with(|| {
                json!({
                    "fingerprint": fingerprint,
                    "rule": rule,
                    "source": source,
                    "target": target,
                    "edge": edge
                })
            });
        }
    }
    output.into_values().collect()
}

fn matching_rules<'contract>(
    contract: &'contract Value,
    from: &str,
    to: &str,
    kind: &str,
) -> Vec<&'contract Value> {
    contract
        .get("dependencyRules")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter(|rule| rule["action"] == "forbid")
        .filter(|rule| list_contains(rule.get("from"), from))
        .filter(|rule| list_contains(rule.get("to"), to))
        .filter(|rule| {
            list_contains(rule.get("kinds"), "any") || list_contains(rule.get("kinds"), kind)
        })
        .collect()
}

fn rules_for_file<'contract>(contract: &'contract Value, file: &str) -> Vec<&'contract Value> {
    let Some(component) = component_for(contract, file) else {
        return Vec::new();
    };
    contract
        .get("dependencyRules")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter(|rule| {
            list_contains(rule.get("from"), component) || list_contains(rule.get("to"), component)
        })
        .collect()
}

fn component_for<'contract>(contract: &'contract Value, file: &str) -> Option<&'contract str> {
    contract
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

fn node_file(node: &weavatrix_graph::Node) -> Option<&str> {
    node.span
        .as_ref()
        .map(|span| span.file.as_str())
        .or_else(|| (node.kind == weavatrix_graph::NodeKind::File).then_some(node.label.as_str()))
}

fn list_contains(value: Option<&Value>, expected: &str) -> bool {
    value
        .and_then(Value::as_array)
        .is_some_and(|values| values.iter().any(|value| value.as_str() == Some(expected)))
}

fn stable_hash(value: &str) -> String {
    let mut hash = 0xcbf2_9ce4_8422_2325_u64;
    for byte in value.bytes() {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    format!("{hash:016x}")
}
