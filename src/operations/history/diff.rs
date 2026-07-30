use super::revision::revision_graph;
use crate::analyzer::Analyzer;
use crate::engine::RepositoryState;
use crate::operations::{arg_str, optional_str, optional_u64};
use blazingly_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};
use weavatrix_git::Repository;
use weavatrix_graph::{Edge, Graph, Node};

pub(super) fn graph_diff(state: &RepositoryState, args: &Value) -> Result<Value, String> {
    let repository = Repository::open(state.root()).map_err(|error| error.to_string())?;
    let base_ref = arg_str(args, "base_ref")?;
    let base = super::resolve_revision(&repository, base_ref)?;
    let analyzer = Analyzer::default();
    let baseline = revision_graph(&analyzer, &repository, state, base)?;
    let max = usize::try_from(optional_u64(args, "max_results")?.unwrap_or(100))
        .map_err(|_| "max_results is too large")?;

    if let Some(head_ref) = optional_str(args, "head_ref")? {
        let head = super::resolve_revision(&repository, head_ref)?;
        let target = revision_graph(&analyzer, &repository, state, head)?;
        let base_text = base.to_string();
        let head_text = head.to_string();
        return Ok(compare(
            &baseline,
            &target,
            &base_text,
            &head_text,
            "immutable_git_revision",
            max,
        ));
    }

    let base_text = base.to_string();
    Ok(compare(
        &baseline,
        state.graph(),
        &base_text,
        "WORKTREE",
        "analyzed_worktree",
        max,
    ))
}

fn compare(
    baseline: &Graph,
    target: &Graph,
    base: &str,
    head: &str,
    target_kind: &str,
    max: usize,
) -> Value {
    let base_nodes = node_map(baseline);
    let target_nodes = node_map(target);
    let added_nodes = target_nodes
        .iter()
        .filter(|(id, _)| !base_nodes.contains_key(**id))
        .map(|(_, node)| *node)
        .collect::<Vec<_>>();
    let removed_nodes = base_nodes
        .iter()
        .filter(|(id, _)| !target_nodes.contains_key(**id))
        .map(|(_, node)| *node)
        .collect::<Vec<_>>();
    let changed_nodes = target_nodes
        .iter()
        .filter_map(|(id, node)| {
            base_nodes
                .get(id)
                .filter(|baseline| nodes_differ(baseline, node))
                .map(|baseline| json!({"before": baseline, "after": node}))
        })
        .collect::<Vec<_>>();
    let base_edges = baseline.edges().iter().collect::<BTreeSet<_>>();
    let target_edges = target.edges().iter().collect::<BTreeSet<_>>();
    let added_edges = target_edges
        .difference(&base_edges)
        .copied()
        .collect::<Vec<_>>();
    let removed_edges = base_edges
        .difference(&target_edges)
        .copied()
        .collect::<Vec<_>>();
    json!({
        "status": "COMPLETE",
        "git_evidence": {"present": true},
        "base": base,
        "head": head,
        "target_kind": target_kind,
        "counts": {
            "nodes_added": added_nodes.len(),
            "nodes_removed": removed_nodes.len(),
            "nodes_changed": changed_nodes.len(),
            "edges_added": added_edges.len(),
            "edges_removed": removed_edges.len()
        },
        "nodes": {
            "added": added_nodes.into_iter().take(max).collect::<Vec<_>>(),
            "removed": removed_nodes.into_iter().take(max).collect::<Vec<_>>(),
            "changed": changed_nodes.into_iter().take(max).collect::<Vec<_>>()
        },
        "edges": {
            "added": added_edges.into_iter().take(max).collect::<Vec<&Edge>>(),
            "removed": removed_edges.into_iter().take(max).collect::<Vec<&Edge>>()
        },
        "completeness": "COMPLETE_FOR_SUPPORTED_LANGUAGES",
        "source_mutation": "NONE",
        "git_process": "NONE"
    })
}

fn node_map(graph: &Graph) -> BTreeMap<&str, &Node> {
    graph
        .nodes()
        .iter()
        .map(|node| (node.id.as_str(), node))
        .collect()
}

/// Immutable Git snapshots carry blob IDs while the scanner carries SHA-256
/// content fingerprints. Both are valid evidence, but comparing their raw
/// strings would mark every unchanged file node as structurally changed.
fn nodes_differ(baseline: &Node, target: &Node) -> bool {
    if baseline.kind != target.kind
        || baseline.id != target.id
        || baseline.label != target.label
        || baseline.language != target.language
        || baseline.span != target.span
    {
        return true;
    }
    let baseline_len = baseline
        .attributes
        .keys()
        .filter(|name| name.as_str() != "content_hash")
        .count();
    let target_len = target
        .attributes
        .keys()
        .filter(|name| name.as_str() != "content_hash")
        .count();
    baseline_len != target_len
        || baseline.attributes.iter().any(|(name, value)| {
            name.as_str() != "content_hash" && target.attributes.get(name) != Some(value)
        })
}
