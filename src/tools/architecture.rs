use crate::RepositoryState;
use crate::tools::{arg_str, node_path};
use blazingly_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::io::ErrorKind;

pub fn contract(state: &RepositoryState, args: &Value) -> Result<Value, String> {
    if arg_str(args, "action").ok() == Some("approve") {
        return Err("read-only MCP never writes architecture contracts".to_owned());
    }
    match load_optional(state)? {
        Some(contract) => Ok(json!({
            "state": "CONFIGURED",
            "source": ".weavatrix/architecture.json",
            "contract": contract
        })),
        None if arg_str(args, "action").ok() == Some("preview") => Ok(json!({
            "state": "PREVIEW",
            "source": "derived graph folders",
            "contract": starter(state),
            "warning": "no active architecture contract",
            "write": "NONE"
        })),
        None => Ok(not_configured(
            state,
            "Save the starter as .weavatrix/architecture.json, review it, then verify.",
        )),
    }
}

pub fn prepare(state: &RepositoryState, args: &Value) -> Result<Value, String> {
    let files = args
        .get("files")
        .and_then(Value::as_array)
        .ok_or_else(|| "files must be an array".to_owned())?;
    let Some(contract) = load_optional(state)? else {
        return Ok(json!({
            "state": "NOT_CONFIGURED",
            "guidance": "PROVISIONAL_STARTER",
            "enforceable": false,
            "intent": args.get("intent"),
            "files": files,
            "starter": starter(state),
            "remediation": remediation(),
            "write": "NONE"
        }));
    };
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
    let Some(contract) = load_optional(state)? else {
        return Ok(json!({
            "state": "NOT_CONFIGURED",
            "enforceable": false,
            "new": [],
            "existing": [],
            "excepted": [],
            "fixed": [],
            "starter": starter(state),
            "remediation": remediation(),
            "write": "NONE"
        }));
    };
    validate_kinds(&contract)?;
    let baseline = contract
        .pointer("/ratchet/baseline/fingerprints")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .collect::<BTreeSet<_>>();
    let accepted = accepted_exceptions(&contract);
    let violations = violations(state, &contract);
    let present = violations
        .iter()
        .filter_map(|item| item["fingerprint"].as_str().map(str::to_owned))
        .collect::<BTreeSet<_>>();
    // An exception a human already accepted must not block; it stays visible
    // so the acceptance is auditable rather than invisible.
    let (excepted, active): (Vec<_>, Vec<_>) = violations
        .into_iter()
        .partition(|item| accepted.contains(item["fingerprint"].as_str().unwrap_or_default()));
    let (existing, new): (Vec<_>, Vec<_>) = active
        .into_iter()
        .partition(|item| baseline.contains(item["fingerprint"].as_str().unwrap_or_default()));
    // Baselined violations that no longer occur: the ratchet can be tightened.
    let fixed = baseline
        .iter()
        .filter(|fingerprint| !present.contains(**fingerprint))
        .collect::<Vec<_>>();
    Ok(json!({
        "state": if new.is_empty() {"PASS"} else {"BLOCKED"},
        "new": new,
        "existing": existing,
        "excepted": excepted,
        "fixed": fixed,
        "contract": ".weavatrix/architecture.json"
    }))
}

/// Fingerprints an accepted, unexpired contract exception covers.
fn accepted_exceptions(contract: &Value) -> BTreeSet<String> {
    contract
        .get("exceptions")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter(|exception| {
            // Without a clock an expiry cannot be evaluated, so a dated
            // exception is honoured only when explicitly marked active.
            exception.get("expires").is_none() || exception["active"] == Value::Bool(true)
        })
        .filter_map(|exception| exception.get("fingerprint")?.as_str().map(str::to_owned))
        .collect()
}

pub fn explain(state: &RepositoryState, args: &Value) -> Result<Value, String> {
    let contract = match configured_contract(
        state,
        "Create and verify an architecture contract before explaining violations.",
    )? {
        LoadedContract::Configured(contract) => contract,
        LoadedContract::NotConfigured(response) => return Ok(response),
    };
    let fingerprint = arg_str(args, "fingerprint")?;
    let Some(violation) = active_violation(state, &contract, fingerprint) else {
        return Ok(json!({
            "state": "NOT_FOUND",
            "fingerprint": fingerprint,
            "reason": "the fingerprint is not an active architecture violation"
        }));
    };
    Ok(json!({"violation": violation, "contract": ".weavatrix/architecture.json"}))
}

pub fn propose_exception(state: &RepositoryState, args: &Value) -> Result<Value, String> {
    let contract = match configured_contract(
        state,
        "Create and verify an architecture contract before proposing exceptions.",
    )? {
        LoadedContract::Configured(contract) => contract,
        LoadedContract::NotConfigured(response) => return Ok(response),
    };
    let fingerprint = arg_str(args, "fingerprint")?;
    let reason = arg_str(args, "reason")?;
    let Some(violation) = active_violation(state, &contract, fingerprint) else {
        return Ok(json!({
            "state": "NOT_FOUND",
            "fingerprint": fingerprint,
            "reason": "only an active architecture violation can be proposed as an exception"
        }));
    };
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

enum LoadedContract {
    Configured(Value),
    NotConfigured(Value),
}

fn configured_contract(state: &RepositoryState, reason: &str) -> Result<LoadedContract, String> {
    Ok(match load_optional(state)? {
        Some(contract) => LoadedContract::Configured(contract),
        None => LoadedContract::NotConfigured(not_configured(state, reason)),
    })
}

fn active_violation(state: &RepositoryState, contract: &Value, fingerprint: &str) -> Option<Value> {
    violations(state, contract)
        .into_iter()
        .find(|item| item["fingerprint"] == fingerprint)
}

fn load_optional(state: &RepositoryState) -> Result<Option<Value>, String> {
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

fn remediation() -> Value {
    json!({
        "offline_path": ".weavatrix/architecture.json",
        "next_tool": "verify_architecture"
    })
}

fn not_configured(state: &RepositoryState, reason: &str) -> Value {
    json!({
        "state": "NOT_CONFIGURED",
        "reason": reason,
        "starter": starter(state),
        "remediation": remediation(),
        "write": "NONE"
    })
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
        let Some(source_file) = node_path(source) else {
            continue;
        };
        let Some(target_file) = node_path(target) else {
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
        for rule in matching_rules(contract, from, to, edge) {
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

/// Coupling vocabulary a rule may name, beside the raw relation names.
const COUPLING_KINDS: &[&str] = &["any", "runtime", "type-only"];

/// Relation names a rule may name. `EdgeKind` accepts any string as a custom
/// kind, so the vocabulary is listed explicitly: otherwise a typo would be
/// taken as a kind that matches nothing and the verification would pass.
const RELATION_KINDS: &[&str] = &[
    "contains",
    "imports",
    "calls",
    "references",
    "method",
    "implements",
    "re_exports",
    "depends_on",
    "inherits",
    "publishes",
    "consumes",
    "binds",
    "reads",
    "writes",
    "deploys",
    "exposes",
    "mounts",
    "configures",
];

/// Rejects a contract whose `kinds` vocabulary this engine cannot evaluate.
///
/// Silently matching nothing would turn an unsupported rule into a passing
/// verification, which is the one failure mode a guard must never have.
fn validate_kinds(contract: &Value) -> Result<(), String> {
    let mut unsupported = BTreeSet::new();
    for rule in contract
        .get("dependencyRules")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        for kind in rule
            .get("kinds")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
        {
            if !COUPLING_KINDS.contains(&kind) && !RELATION_KINDS.contains(&kind) {
                unsupported.insert(kind.to_owned());
            }
        }
    }
    if unsupported.is_empty() {
        return Ok(());
    }
    Err(format!(
        "architecture contract uses dependency kinds this engine cannot evaluate: {}. \
         Supported values are {} plus relation names such as imports, calls, inherits. \
         Rules are rejected rather than skipped, because a rule that matches nothing \
         would report a passing verification.",
        unsupported.into_iter().collect::<Vec<_>>().join(", "),
        COUPLING_KINDS.join(", ")
    ))
}

/// Whether a rule's `kinds` list selects this edge. `runtime` and `type-only`
/// describe how the dependency survives compilation; relation names select the
/// graph relation directly.
fn rule_selects_edge(rule: &Value, edge: &weavatrix_graph::Edge) -> bool {
    let coupling = match edge.attributes.get("coupling") {
        Some(weavatrix_graph::AttributeValue::String(value)) => value.as_str(),
        // Relations other than imports have no compile-time-only form.
        _ => "runtime",
    };
    list_contains(rule.get("kinds"), "any")
        || list_contains(rule.get("kinds"), coupling)
        || list_contains(rule.get("kinds"), edge.kind.as_str())
}

fn matching_rules<'contract>(
    contract: &'contract Value,
    from: &str,
    to: &str,
    edge: &weavatrix_graph::Edge,
) -> Vec<&'contract Value> {
    contract
        .get("dependencyRules")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter(|rule| rule["action"] == "forbid")
        .filter(|rule| list_contains(rule.get("from"), from))
        .filter(|rule| list_contains(rule.get("to"), to))
        .filter(|rule| rule_selects_edge(rule, edge))
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
