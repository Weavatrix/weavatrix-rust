use super::contract::{component_for, list_contains};
use super::policy_diagnostics;
use super::policy_reachability;
use crate::engine::RepositoryState;
use crate::operations::node_path;
use blazingly_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};

const COUPLING_KINDS: &[&str] = &["any", "runtime", "type-only"];
const ACTIONS: &[&str] = &["allow_only", "forbid", "require"];
const REACHABILITY: &[&str] = &["direct", "transitive"];
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
    "unresolved",
];

pub(super) fn dependency_violations(state: &RepositoryState, value: &Value) -> Vec<Value> {
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
        let Some(from) = component_for(value, source_file) else {
            continue;
        };
        let to = component_for(value, target_file);
        if to == Some(from) {
            continue;
        }
        for rule in matching_rules(value, from, to, edge) {
            let identity = format!(
                "{}|{}|{}|{}",
                rule["id"].as_str().unwrap_or("rule"),
                edge.source,
                edge.target,
                edge.kind.as_str()
            );
            let fingerprint = stable_hash(&identity);
            output.entry(fingerprint.clone()).or_insert_with(|| {
                direct_violation(&fingerprint, rule, source, target, edge, from, to)
            });
        }
    }
    for violation in policy_reachability::violations(state, value) {
        let Some(fingerprint) = violation.get("fingerprint").and_then(Value::as_str) else {
            continue;
        };
        output.insert(fingerprint.to_owned(), violation);
    }
    for violation in policy_diagnostics::violations(state, value) {
        let Some(fingerprint) = violation.get("fingerprint").and_then(Value::as_str) else {
            continue;
        };
        output.insert(fingerprint.to_owned(), violation);
    }
    output.into_values().collect()
}

pub(super) fn validate(value: &Value) -> Result<(), String> {
    validate_vocabulary(value)?;
    let mut unsupported = BTreeSet::new();
    for rule in value
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

fn validate_vocabulary(value: &Value) -> Result<(), String> {
    let rules = value
        .get("dependencyRules")
        .and_then(Value::as_array)
        .into_iter()
        .flatten();
    let mut actions = BTreeSet::new();
    let mut reachability = BTreeSet::new();
    let mut invalid_combinations = BTreeSet::new();
    let mut invalid_unresolved = BTreeSet::new();
    for rule in rules {
        let action = rule
            .get("action")
            .and_then(Value::as_str)
            .unwrap_or("(missing)");
        if !ACTIONS.contains(&action) {
            actions.insert(action.to_owned());
        }
        if let Some(value) = rule.get("reachability") {
            let value = value.as_str().unwrap_or("(non-string)");
            if !REACHABILITY.contains(&value) {
                reachability.insert(value.to_owned());
            }
        }
        let mode = rule
            .get("reachability")
            .and_then(Value::as_str)
            .unwrap_or("direct");
        if action == "allow_only" && mode != "direct" {
            invalid_combinations.insert(rule["id"].as_str().unwrap_or("rule").to_owned());
        }
        if list_contains(rule.get("kinds"), "unresolved")
            && (action != "forbid" || mode != "direct")
        {
            invalid_unresolved.insert(rule["id"].as_str().unwrap_or("rule").to_owned());
        }
    }
    if !actions.is_empty() {
        return Err(format!(
            "unsupported architecture rule actions: {}. Supported actions are {}",
            actions.into_iter().collect::<Vec<_>>().join(", "),
            ACTIONS.join(", ")
        ));
    }
    if !reachability.is_empty() {
        return Err(format!(
            "unsupported architecture rule reachability: {}. Supported values are {}",
            reachability.into_iter().collect::<Vec<_>>().join(", "),
            REACHABILITY.join(", ")
        ));
    }
    if !invalid_combinations.is_empty() {
        return Err(format!(
            "allow_only supports only direct reachability; invalid rules: {}",
            invalid_combinations
                .into_iter()
                .collect::<Vec<_>>()
                .join(", ")
        ));
    }
    if !invalid_unresolved.is_empty() {
        return Err(format!(
            "unresolved is supported only by direct forbid rules; invalid rules: {}",
            invalid_unresolved
                .into_iter()
                .collect::<Vec<_>>()
                .join(", ")
        ));
    }
    Ok(())
}

fn direct_violation(
    fingerprint: &str,
    rule: &Value,
    source: &weavatrix_graph::Node,
    target: &weavatrix_graph::Node,
    edge: &weavatrix_graph::Edge,
    from: &str,
    to: Option<&str>,
) -> Value {
    if rule["action"] == "allow_only" {
        return json!({
            "fingerprint": fingerprint,
            "category": "dependency",
            "rule": rule,
            "source": source,
            "target": target,
            "edge": edge,
            "evidence": {
                "kind": "dependency_outside_allow_list",
                "source_component": from,
                "target_component": to.unwrap_or("(unmapped)"),
                "allowed_components": rule["to"]
            }
        });
    }
    json!({
        "fingerprint": fingerprint,
        "rule": rule,
        "source": source,
        "target": target,
        "edge": edge
    })
}

pub(super) fn stable_hash(value: &str) -> String {
    let mut hash = 0xcbf2_9ce4_8422_2325_u64;
    for byte in value.bytes() {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    format!("{hash:016x}")
}

pub(super) fn rule_selects_edge(rule: &Value, edge: &weavatrix_graph::Edge) -> bool {
    let coupling = match edge.attributes.get("coupling") {
        Some(weavatrix_graph::AttributeValue::String(value)) => value.as_str(),
        _ => "runtime",
    };
    list_contains(rule.get("kinds"), "any")
        || list_contains(rule.get("kinds"), coupling)
        || list_contains(rule.get("kinds"), edge.kind.as_str())
}

fn matching_rules<'contract>(
    value: &'contract Value,
    from: &str,
    to: Option<&str>,
    edge: &weavatrix_graph::Edge,
) -> Vec<&'contract Value> {
    value
        .get("dependencyRules")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter(|rule| rule["action"] == "forbid" || rule["action"] == "allow_only")
        .filter(|rule| {
            rule.get("reachability")
                .and_then(Value::as_str)
                .unwrap_or("direct")
                == "direct"
        })
        .filter(|rule| list_contains(rule.get("from"), from))
        .filter(|rule| match rule["action"].as_str() {
            Some("forbid") => to.is_some_and(|component| list_contains(rule.get("to"), component)),
            Some("allow_only") => {
                to.is_none_or(|component| !list_contains(rule.get("to"), component))
            }
            _ => false,
        })
        .filter(|rule| rule_selects_edge(rule, edge))
        .collect()
}
