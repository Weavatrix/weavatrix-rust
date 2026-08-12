//! Path-selector dependency rules and strict rule-shape validation.
//!
//! A rule addresses files either through declared components (`from`/`to`)
//! or directly through path selectors (`fromPath`/`toPath`, with optional
//! `fromPathNot`/`toPathNot` exclusions), never both at once. Selector rules
//! are Dependency-Cruiser-shaped: patterns use the declared subset compiled
//! by `path_pattern`, the to side may reference `fromPath` capture groups as
//! `$1`..`$9`, and anything the engine cannot evaluate rejects the contract
//! instead of silently selecting nothing.

use super::path_pattern::PathPattern;
use super::policy_templates::{Target, TargetCache, TargetSelector};
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
        patterns.extend(pattern_failures(rule, id));
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

fn pattern_failures(rule: &Value, id: &str) -> Vec<String> {
    let mut failures = Vec::new();
    let mut from_groups = None;
    for key in ["fromPath", "fromPathNot"] {
        let Some(raw) = string_pattern(rule, key, id, &mut failures) else {
            continue;
        };
        match PathPattern::compile(raw) {
            Ok(pattern) => {
                if key == "fromPath" {
                    from_groups = Some(pattern.group_count());
                }
            }
            Err(error) => failures.push(format!("{id}.{key}: {error}")),
        }
    }
    for key in ["toPath", "toPathNot"] {
        let Some(raw) = string_pattern(rule, key, id, &mut failures) else {
            continue;
        };
        let template = Target::parse(raw);
        if template.max_ref() == 0 {
            if let Err(error) = PathPattern::compile(raw) {
                failures.push(format!("{id}.{key}: {error}"));
            }
            continue;
        }
        let Some(groups) = from_groups else {
            failures.push(format!(
                "{id}.{key} uses group references but the rule has no fromPath"
            ));
            continue;
        };
        if template.max_ref() > groups {
            failures.push(format!(
                "{id}.{key} references ${} but fromPath captures only {groups} group(s)",
                template.max_ref()
            ));
            continue;
        }
        if let Err(error) = PathPattern::compile(&template.instantiate(&vec![None; groups])) {
            failures.push(format!("{id}.{key}: {error}"));
        }
    }
    failures
}

fn string_pattern<'rule>(
    rule: &'rule Value,
    key: &str,
    id: &str,
    failures: &mut Vec<String>,
) -> Option<&'rule str> {
    let value = rule.get(key)?;
    if let Some(raw) = value.as_str() {
        Some(raw)
    } else {
        failures.push(format!("{id}.{key} must be a string pattern"));
        None
    }
}

pub(super) fn violations(state: &RepositoryState, value: &Value) -> Vec<Value> {
    let compiled: Vec<(&Value, SourceSelector, TargetSelector)> = rules(value)
        .filter(|rule| selects_by_path(rule))
        .filter_map(|rule| {
            Some((
                rule,
                SourceSelector::compile(rule)?,
                TargetSelector::compile(rule)?,
            ))
        })
        .collect();
    if compiled.is_empty() {
        return Vec::new();
    }
    let mut cache = TargetCache::new();
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
            let Some(captures) = from.captures(source_file) else {
                continue;
            };
            if !to.selects(target_file, &captures, &mut cache) || !rule_selects_edge(rule, edge) {
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

struct SourceSelector {
    include: Option<PathPattern>,
    exclude: Option<PathPattern>,
}

impl SourceSelector {
    /// Returns `None` only for patterns `validate` already rejected.
    fn compile(rule: &Value) -> Option<Self> {
        Some(Self {
            include: compiled_pattern(rule, "fromPath").ok()?,
            exclude: compiled_pattern(rule, "fromPathNot").ok()?,
        })
    }

    /// Returns the include captures when the path is selected.
    fn captures(&self, path: &str) -> Option<Vec<Option<String>>> {
        if self
            .exclude
            .as_ref()
            .is_some_and(|pattern| pattern.matches(path).is_some())
        {
            return None;
        }
        match &self.include {
            Some(pattern) => pattern.matches(path),
            None => Some(Vec::new()),
        }
    }
}

/// `Err` marks a pattern `validate` already rejected; the selector is skipped.
fn compiled_pattern(rule: &Value, key: &str) -> Result<Option<PathPattern>, ()> {
    match rule.get(key).map(Value::as_str) {
        None => Ok(None),
        Some(Some(pattern)) => match PathPattern::compile(pattern) {
            Ok(compiled) => Ok(Some(compiled)),
            Err(_) => Err(()),
        },
        Some(None) => Err(()),
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
