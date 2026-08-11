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
    // Five arrays share this cap, and a node carries its attributes and span,
    // so a hundred each is a six-figure token answer for a ten-commit range.
    // `counts` stays exact, so a smaller sample hides nothing.
    let max = usize::try_from(optional_u64(args, "max_results")?.unwrap_or(25))
        .map_err(|_| "max_results is too large")?;
    let budget = crate::operations::token_budget::requested(args)?;
    let detailed = match optional_str(args, "detail")?.unwrap_or("file_pairs") {
        "file_pairs" => false,
        "edges" => true,
        other => {
            return Err(format!(
                "unknown detail {other:?}; expected file_pairs or edges"
            ));
        }
    };

    let mut report = if let Some(head_ref) = optional_str(args, "head_ref")? {
        let head = super::resolve_revision(&repository, head_ref)?;
        let target = revision_graph(&analyzer, &repository, state, head)?;
        let base_text = base.to_string();
        let head_text = head.to_string();
        compare(
            &baseline,
            &target,
            &base_text,
            &head_text,
            "immutable_git_revision",
            max,
            detailed,
        )
    } else {
        let base_text = base.to_string();
        compare(
            &baseline,
            state.graph(),
            &base_text,
            "WORKTREE",
            "analyzed_worktree",
            max,
            detailed,
        )
    };
    crate::operations::token_budget::fit(
        &mut report,
        budget,
        &[
            "/nodes/changed",
            "/edges/by_file",
            "/edges/added",
            "/edges/removed",
            "/nodes/added",
            "/nodes/removed",
        ],
    );
    Ok(report)
}

fn compare(
    baseline: &Graph,
    target: &Graph,
    base: &str,
    head: &str,
    target_kind: &str,
    max: usize,
    detailed: bool,
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
    let edges = if detailed {
        json!({
            "detail": "edges",
            "added": added_edges.iter().copied().take(max).collect::<Vec<&Edge>>(),
            "removed": removed_edges.iter().copied().take(max).collect::<Vec<&Edge>>()
        })
    } else {
        // One edge serializes to roughly three hundred bytes of provenance,
        // and a ten-commit range moves thousands of them: on this repository
        // the edge lists were 88% of the whole answer. Rolled up to the file
        // pairs they connect, the same churn is the structural story a caller
        // asked for, and it is an order of magnitude smaller.
        json!({
            "detail": "file_pairs",
            "by_file": file_pairs(baseline, target, &added_edges, &removed_edges, max),
            "note": "pass detail=edges for individual edges with provenance"
        })
    };
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
        "edges": edges,
        "completeness": "COMPLETE_FOR_SUPPORTED_LANGUAGES",
        "source_mutation": "NONE",
        "git_process": "NONE"
    })
}

/// Edge churn rolled up to the files the endpoints live in.
///
/// A moved symbol re-creates every edge it owns, so the raw delta is dominated
/// by churn that says nothing a caller can act on. The file pair does: it is
/// the coupling that appeared or disappeared between two revisions.
fn file_pairs(
    baseline: &Graph,
    target: &Graph,
    added: &[&Edge],
    removed: &[&Edge],
    max: usize,
) -> Vec<Value> {
    let mut pairs = BTreeMap::<(String, String, String), (usize, usize)>::new();
    for (edges, added_side) in [(added, true), (removed, false)] {
        for edge in edges {
            let key = (
                owning_file(baseline, target, edge.source.as_str()),
                owning_file(baseline, target, edge.target.as_str()),
                edge.kind.as_str().to_owned(),
            );
            let counts = pairs.entry(key).or_insert((0, 0));
            if added_side {
                counts.0 += 1;
            } else {
                counts.1 += 1;
            }
        }
    }
    let mut rolled = pairs.into_iter().collect::<Vec<_>>();
    rolled.sort_by(|left, right| {
        (right.1.0 + right.1.1)
            .cmp(&(left.1.0 + left.1.1))
            .then_with(|| left.0.cmp(&right.0))
    });
    rolled
        .into_iter()
        .take(max)
        .map(|((from, to, relation), (added, removed))| {
            json!({
                "from": from,
                "to": to,
                "relation": relation,
                "added": added,
                "removed": removed
            })
        })
        .collect()
}

/// The file a node's evidence comes from, in whichever revision still has it.
fn owning_file(baseline: &Graph, target: &Graph, id: &str) -> String {
    for graph in [target, baseline] {
        if let Some(node) = graph.node(id) {
            if let Some(path) = crate::operations::node_path(node) {
                return path.to_owned();
            }
            return node.label.clone();
        }
    }
    id.to_owned()
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
