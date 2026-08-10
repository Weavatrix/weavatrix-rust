//! Verification of the declared served surface against extracted evidence.
//!
//! The contract's `capabilities` section is a mapping: each entry claims that
//! some named capability is served by a set of endpoints, optionally from named
//! components. Nothing in it is evidence. What makes it worth having is that
//! every entry is resolvable against the graph at one revision, so a claim that
//! stopped being true is a finding rather than a comment nobody re-read.
//!
//! Four states cover the mapping in both directions. `served` resolved. A
//! `drifted` capability still resolves, but out of the components it named.
//! An `orphaned` capability names an endpoint this revision does not expose.
//! An `unmapped` endpoint is exposed and claimed by nothing, which is the half
//! a declared catalog cannot see about itself.

use super::contract::component_for;
use super::rules::stable_hash;
use crate::engine::RepositoryState;
use crate::operations::node_path;
use blazingly_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};
use weavatrix_graph::{NodeIndex, NodeKind};

/// Unclaimed endpoints and the derived starter grow with the repository, not
/// with the contract, so both are trimmed by default and always report the
/// total they were trimmed from.
pub(super) const DEFAULT_MAX_RESULTS: u64 = 50;

pub(super) struct Report {
    pub served: Vec<Value>,
    pub drifted: Vec<Value>,
    pub orphaned: Vec<Value>,
    pub unmapped: Vec<Value>,
}

impl Report {
    pub(super) fn blocked(&self) -> bool {
        !self.drifted.is_empty() || !self.orphaned.is_empty()
    }
}

/// Keeps the first `limit` entries, or all of them when the caller passes 0.
pub(super) fn trimmed(values: Vec<Value>, limit: u64) -> (Vec<Value>, usize) {
    let total = values.len();
    if limit == 0 {
        return (values, total);
    }
    let keep = usize::try_from(limit).unwrap_or(usize::MAX);
    (values.into_iter().take(keep).collect(), total)
}

pub(super) fn declared(value: &Value) -> Option<&Vec<Value>> {
    value.get("capabilities")?.as_array()
}

/// Rejects a capability the engine cannot resolve rather than skipping it. A
/// skipped entry would report a passing verification for a claim nothing
/// checked, which is worse than having made no claim.
pub(super) fn validate(value: &Value) -> Result<(), String> {
    let mut nameless = 0_usize;
    let mut endpointless = BTreeSet::new();
    let mut duplicated = BTreeSet::new();
    let mut seen = BTreeSet::new();
    for capability in declared(value).into_iter().flatten() {
        let Some(id) = capability.get("id").and_then(Value::as_str) else {
            nameless += 1;
            continue;
        };
        if !seen.insert(id) {
            duplicated.insert(id.to_owned());
        }
        if endpoints_of(capability).next().is_none() {
            endpointless.insert(id.to_owned());
        }
    }
    if nameless > 0 {
        return Err(format!(
            "architecture contract declares {nameless} capabilities without an `id`. \
             An unnamed capability cannot be reported against, so it is rejected \
             rather than verified silently."
        ));
    }
    if !duplicated.is_empty() {
        return Err(format!(
            "architecture contract declares the same capability id more than once: {}. \
             Two entries under one id cannot both be reported, so the duplicate is \
             rejected rather than resolved by order.",
            joined(&duplicated)
        ));
    }
    if !endpointless.is_empty() {
        return Err(format!(
            "architecture contract declares capabilities with no `endpoints`: {}. \
             This engine resolves a capability through the endpoints it serves; \
             an entry with none would pass by having claimed nothing.",
            joined(&endpointless)
        ));
    }
    Ok(())
}

pub(super) fn verify(state: &RepositoryState, value: &Value, args: &Value) -> Report {
    let exposed = exposed_endpoints(state, args);
    let mut claimed = BTreeSet::new();
    let mut served = Vec::new();
    let mut drifted = Vec::new();
    let mut orphaned = Vec::new();

    for capability in declared(value).into_iter().flatten() {
        let id = capability.get("id").and_then(Value::as_str).unwrap_or("");
        let required = required_components(capability);
        let mut resolved = Vec::new();
        let mut missing = Vec::new();
        let mut outside = Vec::new();
        for endpoint in endpoints_of(capability) {
            claimed.insert(endpoint.to_owned());
            let Some(files) = exposed.get(endpoint) else {
                missing.push(endpoint.to_owned());
                continue;
            };
            let components = components_of(value, files);
            if required.is_empty() || components.iter().any(|item| required.contains(item)) {
                resolved.push(json!({"endpoint": endpoint, "components": as_array(&components)}));
            } else {
                outside.push(json!({
                    "endpoint": endpoint,
                    "declared_in": as_array(&components),
                    "contract_expects": as_array(&required)
                }));
            }
        }
        if !missing.is_empty() {
            orphaned.push(finding(
                "capability.orphaned",
                id,
                &json!({"endpoints": missing}),
            ));
        }
        if !outside.is_empty() {
            drifted.push(finding(
                "capability.drifted",
                id,
                &json!({"endpoints": outside}),
            ));
        }
        if missing.is_empty() && outside.is_empty() {
            served.push(json!({"id": id, "endpoints": resolved}));
        }
    }

    let unmapped = exposed
        .iter()
        .filter(|(endpoint, _)| !claimed.contains(*endpoint))
        .map(|(endpoint, files)| {
            json!({"endpoint": endpoint, "components": as_array(&components_of(value, files))})
        })
        .collect();

    Report {
        served,
        drifted,
        orphaned,
        unmapped,
    }
}

/// One derived capability per exposed endpoint: the identity mapping a
/// repository already satisfies, offered as the starting point for a contract
/// the reader then narrows by hand.
pub(super) fn starter(state: &RepositoryState, args: &Value) -> Vec<Value> {
    exposed_endpoints(state, args)
        .into_iter()
        .map(|(endpoint, files)| {
            json!({
                "id": endpoint.to_ascii_lowercase().replace([' ', '/'], "."),
                "name": endpoint,
                "endpoints": [&endpoint],
                "files": as_array(&files)
            })
        })
        .collect()
}

/// Endpoint label to the files that declare it. An endpoint node carries no
/// span of its own; the files that expose it are its incoming evidence, which
/// is the same route `node_is_visible` takes to classify one.
///
/// A served surface is what production serves. A route declared only in a test
/// fixture is evidence of a test, so it neither needs claiming nor counts as
/// unclaimed until `include_tests` asks for it.
fn exposed_endpoints(state: &RepositoryState, args: &Value) -> BTreeMap<String, BTreeSet<String>> {
    let mut exposed = BTreeMap::<String, BTreeSet<String>>::new();
    for (slot, node) in state.graph().nodes().iter().enumerate() {
        if node.kind != NodeKind::Endpoint || !crate::operations::node_is_visible(state, slot, args)
        {
            continue;
        }
        let index = NodeIndex::new(u32::try_from(slot).unwrap_or(u32::MAX));
        let files = exposed.entry(node.label.clone()).or_default();
        for edge in state.graph().incoming_at(index) {
            let Some(source) = state.graph().node(edge.source.as_str()) else {
                continue;
            };
            if let Some(path) = node_path(source) {
                files.insert(path.to_owned());
            }
        }
    }
    exposed
}

fn components_of(value: &Value, files: &BTreeSet<String>) -> BTreeSet<String> {
    files
        .iter()
        .filter_map(|file| component_for(value, file))
        .map(str::to_owned)
        .collect()
}

fn required_components(capability: &Value) -> BTreeSet<String> {
    capability
        .get("components")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .map(str::to_owned)
        .collect()
}

fn endpoints_of(capability: &Value) -> impl Iterator<Item = &str> {
    capability
        .get("endpoints")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
}

fn finding(code: &str, id: &str, evidence: &Value) -> Value {
    json!({
        "fingerprint": stable_hash(&format!("{code}|{id}|{evidence}")),
        "code": code,
        "capability": id,
        "evidence": evidence
    })
}

fn as_array(values: &BTreeSet<String>) -> Vec<&str> {
    values.iter().map(String::as_str).collect()
}

fn joined(values: &BTreeSet<String>) -> String {
    values
        .iter()
        .map(String::as_str)
        .collect::<Vec<_>>()
        .join(", ")
}
