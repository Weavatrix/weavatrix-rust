//! Path-selector dependency rules and strict rule-shape validation.
//!
//! A rule addresses files either through declared components (`from`/`to`)
//! or directly through path selectors (`fromPath`/`toPath`, with optional
//! `fromPathNot`/`toPathNot` exclusions), never both at once. Selector rules
//! are Dependency-Cruiser-shaped: patterns use the declared subset compiled
//! by `path_pattern`, and anything the engine cannot evaluate rejects the
//! contract instead of silently selecting nothing.

use super::path_pattern::PathPattern;
use super::rules::{rule_selects_edge, stable_hash};
use crate::engine::RepositoryState;
use crate::operations::node_path;
use blazingly_json::{Value, json};
use std::collections::BTreeSet;

const RULE_KEYS: &[&str] = &[
    "id",
    "action",
    "reachability",
    "from",
    "to",
    "fromPath",
    "fromPathNot",
    "toPath",
    "toPathNot",
    "kinds",
    "severity",
    "comment",
];
const SEVERITIES: &[&str] = &["error", "warn"];
const PATH_KEYS: &[&str] = &["fromPath", "fromPathNot", "toPath", "toPathNot"];

pub(super) fn validate(value: &Value) -> Result<(), String> {
    let mut unknown = BTreeSet::new();
    let mut severities = BTreeSet::new();
    let mut mixed = BTreeSet::new();
    let mut unsupported = BTreeSet::new();
    let mut unselective = BTreeSet::new();
    let mut patterns = Vec::new();
    for rule in rules(value) {
        let id = rule_id(rule);
        if let Some(entries) = rule.as_object() {
            for key in entries
                .keys()
                .filter(|key| !RULE_KEYS.contains(&key.as_str()))
            {
                unknown.insert(format!("{id}.{key}"));
            }
        }
        if let Some(severity) = rule.get("severity")
            && !severity
                .as_str()
                .is_some_and(|value| SEVERITIES.contains(&value))
        {
            severities.insert(id.to_owned());
        }
        if !selects_by_path(rule) {
            continue;
        }
        if rule.get("from").is_some() || rule.get("to").is_some() {
            mixed.insert(id.to_owned());
        }
        let reachability = rule
            .get("reachability")
            .and_then(Value::as_str)
            .unwrap_or("direct");
        if rule["action"] != "forbid" || reachability != "direct" {
            unsupported.insert(id.to_owned());
        }
        if rule.get("fromPath").is_none() && rule.get("fromPathNot").is_none()
            || rule.get("toPath").is_none() && rule.get("toPathNot").is_none()
        {
            unselective.insert(id.to_owned());
        }
        for key in PATH_KEYS {
            if let Some(pattern) = rule.get(*key) {
                let Some(pattern) = pattern.as_str() else {
                    patterns.push(format!("{id}.{key} must be a string pattern"));
                    continue;
                };
                if let Err(error) = PathPattern::compile(pattern) {
                    patterns.push(format!("{id}.{key}: {error}"));
                }
            }
        }
    }
    let reject = |failures: BTreeSet<String>, reason: &str| -> Result<(), String> {
        if failures.is_empty() {
            return Ok(());
        }
        Err(format!(
            "{reason}: {}. Rules are rejected rather than skipped, because a rule \
             that matches nothing would report a passing verification.",
            failures.into_iter().collect::<Vec<_>>().join(", ")
        ))
    };
    reject(
        unknown,
        "architecture rules carry fields this engine does not evaluate",
    )?;
    reject(
        severities,
        "architecture rule severity must be `error` or `warn`; invalid rules",
    )?;
    reject(
        mixed,
        "a rule must address components or paths, not both; invalid rules",
    )?;
    reject(
        unsupported,
        "path selectors support only direct forbid rules; invalid rules",
    )?;
    reject(
        unselective,
        "a path rule needs a selector on each side; invalid rules",
    )?;
    if patterns.is_empty() {
        return Ok(());
    }
    Err(patterns.join("; "))
}

pub(super) fn violations(state: &RepositoryState, value: &Value) -> Vec<Value> {
    let compiled: Vec<(&Value, Selector, Selector)> = rules(value)
        .filter(|rule| selects_by_path(rule))
        .filter_map(|rule| {
            Some((
                rule,
                Selector::compile(rule, "fromPath", "fromPathNot")?,
                Selector::compile(rule, "toPath", "toPathNot")?,
            ))
        })
        .collect();
    if compiled.is_empty() {
        return Vec::new();
    }
    let mut output = Vec::new();
    let mut seen = BTreeSet::new();
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
        if source_file == target_file {
            continue;
        }
        for (rule, from, to) in &compiled {
            if !from.selects(source_file)
                || !to.selects(target_file)
                || !rule_selects_edge(rule, edge)
            {
                continue;
            }
            let identity = format!(
                "{}|{}|{}|{}",
                rule_id(rule),
                edge.source,
                edge.target,
                edge.kind.as_str()
            );
            let fingerprint = stable_hash(&identity);
            if !seen.insert(fingerprint.clone()) {
                continue;
            }
            output.push(json!({
                "fingerprint": fingerprint,
                "category": "dependency",
                "rule": rule,
                "source": source,
                "target": target,
                "edge": edge
            }));
        }
    }
    output
}

struct Selector {
    include: Option<PathPattern>,
    exclude: Option<PathPattern>,
}

impl Selector {
    /// Returns `None` only for patterns `validate` already rejected.
    fn compile(rule: &Value, include: &str, exclude: &str) -> Option<Self> {
        let compile = |key: &str| -> Option<Option<PathPattern>> {
            match rule.get(key).map(|pattern| pattern.as_str()) {
                None => Some(None),
                Some(Some(pattern)) => PathPattern::compile(pattern).ok().map(Some),
                Some(None) => None,
            }
        };
        Some(Self {
            include: compile(include)?,
            exclude: compile(exclude)?,
        })
    }

    fn selects(&self, path: &str) -> bool {
        self.include
            .as_ref()
            .is_none_or(|pattern| pattern.matches(path).is_some())
            && self
                .exclude
                .as_ref()
                .is_none_or(|pattern| pattern.matches(path).is_none())
    }
}

fn rules(value: &Value) -> impl Iterator<Item = &Value> {
    value
        .get("dependencyRules")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
}

fn selects_by_path(rule: &Value) -> bool {
    PATH_KEYS.iter().any(|key| rule.get(*key).is_some())
}

fn rule_id(rule: &Value) -> &str {
    rule.get("id").and_then(Value::as_str).unwrap_or("rule")
}
