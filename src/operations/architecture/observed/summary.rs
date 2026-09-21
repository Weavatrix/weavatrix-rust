//! Bounded orientation view of the complete observed graph. Counts are taken
//! before sampling, so this view does not turn a presentation cap into a
//! false assertion about absence of edges or cycles.

use blazingly_json::{Value, json};
use std::collections::BTreeMap;

pub(super) fn from_full(full: &Value) -> Value {
    let paths = component_paths(full);
    let components = component_sample(full);
    let coupling = coupling_sample(full, &paths);
    let cycle_samples = cycle_sample(full, &paths);
    let packages = package_sample(full);
    let runners = runner_sample(full);

    json!({
        "kind": "observed",
        "detail": "summary",
        "analysis_id": full["analysis_id"],
        "view": full["view"],
        "status": full["status"],
        "analysis": full["analysis"],
        "input_capture": {
            "complete": full["input_capture"]["complete"],
            "files_captured": full["input_capture"]["files_captured"],
            "excluded_total": full["input_capture"]["excluded"].as_array().map_or(0, Vec::len)
        },
        "diagnostics": full["diagnostics"],
        "architecture_hypotheses": full["architecture_hypotheses"],
        "packages_total": full["packages"].as_array().map_or(0, Vec::len),
        "packages": packages,
        "packages_sample_truncated": full["packages"].as_array().map_or(0, Vec::len) > 16,
        "components_total": paths.len(),
        "components": components,
        "components_sample_truncated": paths.len() > 12,
        "edges_total": full["edges_total"],
        "internal_connectivity_total": full["internal_connectivity_total"],
        "coupling": coupling,
        "coupling_sample_truncated": full["edges_total"].as_u64().unwrap_or(0) > 8,
        "cycle_candidates": {
            "total": full["cycles"]["cyclic_scc_total"],
            "classification": full["cycles"]["classification"],
            "samples": cycle_samples,
            "samples_truncated": full["cycles"]["cyclic_scc_total"].as_u64().unwrap_or(0) > 3
        },
        "build": {
            "status": full["build_topology"]["status"],
            "workspaces_total": full["build_topology"]["workspaces"].as_array().map_or(0, Vec::len),
            "runners_total": full["build_topology"]["runners"].as_array().map_or(0, Vec::len),
            "runners": runners,
            "runners_sample_truncated": full["build_topology"]["runners"].as_array().map_or(0, Vec::len) > 8
        },
        "interpretation": "Architecture styles are evidence-bounded hypotheses, not the declared target contract. Cycle candidates are quotient/union cycles, not proven runtime cycles.",
        "follow_up": "Use detail=full to inspect all components, paged edges, provenance, and cycle witnesses."
    })
}

fn component_paths(full: &Value) -> BTreeMap<String, String> {
    full["components"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|component| {
            Some((
                component["id"].as_str()?.to_owned(),
                component["path"].as_str()?.to_owned(),
            ))
        })
        .collect()
}

fn component_sample(full: &Value) -> Vec<Value> {
    let mut components = full["components"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|component| {
            json!({
                "path": component["path"],
                "files": component["files"],
                "declared_ids": component["declared_ids"]
            })
        })
        .collect::<Vec<_>>();
    components.sort_by(|left, right| {
        right["files"]
            .as_u64()
            .cmp(&left["files"].as_u64())
            .then_with(|| left["path"].as_str().cmp(&right["path"].as_str()))
    });
    components.truncate(12);
    components
}

fn coupling_sample(full: &Value, paths: &BTreeMap<String, String>) -> Vec<Value> {
    let mut coupling = full["edges"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|edge| {
            json!({
                "from": path(paths, edge["from"].as_str()),
                "to": path(paths, edge["to"].as_str()),
                "relation": edge["relation"],
                "occurrences": edge["occurrence_count"],
                "file_pairs": edge["distinct_file_pairs"]
            })
        })
        .collect::<Vec<_>>();
    coupling.sort_by(|left, right| {
        right["occurrences"]
            .as_u64()
            .cmp(&left["occurrences"].as_u64())
            .then_with(|| left["from"].as_str().cmp(&right["from"].as_str()))
            .then_with(|| left["to"].as_str().cmp(&right["to"].as_str()))
    });
    coupling.truncate(8);
    coupling
}

fn cycle_sample(full: &Value, paths: &BTreeMap<String, String>) -> Vec<Value> {
    full["cycles"]["cyclic_scc"]
        .as_array()
        .into_iter()
        .flatten()
        .take(3)
        .map(|cycle| {
            let members = cycle["members"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(Value::as_str)
                .take(8)
                .map(|id| path(paths, Some(id)))
                .collect::<Vec<_>>();
            let witness = cycle["component_edge_witness"]
                .as_array()
                .into_iter()
                .flatten()
                .take(3)
                .map(|edge| {
                    let evidence = edge["evidence_sample"]
                        .as_array()
                        .and_then(|samples| samples.first());
                    json!({
                        "from": path(paths, edge["from"].as_str()),
                        "to": path(paths, edge["to"].as_str()),
                        "relation": edge["relation"],
                        "source_file": evidence.map(|item| &item["source_file"]),
                        "target_file": evidence.map(|item| &item["target_file"])
                    })
                })
                .collect::<Vec<_>>();
            json!({
                "members": members,
                "member_count": cycle["members"].as_array().map_or(0, Vec::len),
                "witness": witness,
                "lifting_status": cycle["lifting_status"],
                "configuration_status": cycle["configuration_status"]
            })
        })
        .collect()
}

fn package_sample(full: &Value) -> Vec<Value> {
    full["packages"]
        .as_array()
        .into_iter()
        .flatten()
        .take(16)
        .map(|package| {
            json!({
                "name": package["name"],
                "path": package["path"],
                "ecosystem": package["ecosystem"],
                "manifest": package["manifest"]
            })
        })
        .collect()
}

fn runner_sample(full: &Value) -> Vec<Value> {
    full["build_topology"]["runners"]
        .as_array()
        .into_iter()
        .flatten()
        .take(8)
        .map(|runner| json!({"kind": runner["kind"], "path": runner["path"]}))
        .collect()
}

fn path(paths: &BTreeMap<String, String>, id: Option<&str>) -> String {
    id.and_then(|id| paths.get(id)).cloned().unwrap_or_default()
}
