use crate::engine::RepositoryState;
use crate::operations::{arg_str, arg_u64};
use blazingly_json::{Value, json};
use std::collections::BTreeMap;
use weavatrix_graph::{Direction, NodeIndex, NodeKind, shortest_path};

mod pagination;
mod trace;
mod views;
mod walk;

use pagination::page_offset;
pub(super) use trace::endpoint as trace_endpoint;
pub use views::{communities, endpoints, module_map};
use walk::resolve_seeds;
pub(super) use walk::traverse;

pub fn stats(state: &RepositoryState) -> Value {
    let mut kinds = BTreeMap::<String, u64>::new();
    let mut relations = BTreeMap::<String, u64>::new();
    let mut evidence = BTreeMap::<String, u64>::new();
    for node in state.graph().nodes() {
        *kinds.entry(node.kind.as_str().to_owned()).or_default() += 1;
    }
    for edge in state.graph().edges() {
        *relations.entry(edge.kind.as_str().to_owned()).or_default() += 1;
        *evidence
            .entry(edge.provenance.evidence.as_str().to_owned())
            .or_default() += 1;
    }
    json!({
        "repository": state.snapshot().repository,
        "revision": state.snapshot().revision,
        "nodes": state.graph().node_count(),
        "edges": state.graph().edge_count(),
        "build_ms": state.build_time().as_secs_f64() * 1000.0,
        "node_kinds": kinds,
        "relations": relations,
        "evidence": evidence,
        "capabilities": state.snapshot().capabilities,
        "freshness": {
            "state": "CURRENT",
            "source_revision": state.snapshot().revision,
            "incremental_hashes_reused": state.scan_report().cache.reused_hashes,
            "content_reads": state.scan_report().cache.content_reads
        }
    })
}

pub fn get_node(state: &RepositoryState, args: &Value) -> Result<Value, String> {
    let index = state.resolve_node(arg_str(args, "label")?)?;
    let node = state.node(index)?;
    Ok(json!({
        "node": node,
        "incoming": state.graph().in_degree(index),
        "outgoing": state.graph().out_degree(index)
    }))
}

pub fn neighbors(state: &RepositoryState, args: &Value) -> Result<Value, String> {
    let index = state.resolve_node(arg_str(args, "label")?)?;
    let filter = arg_str(args, "relation_filter").ok();
    let offset = page_offset(args)?;
    let max_results = usize::try_from(arg_u64(args, "max_results").unwrap_or(50)).unwrap_or(50);
    if max_results == 0 || max_results > 500 {
        return Err("max_results must be between 1 and 500".to_owned());
    }
    let full = arg_str(args, "response_detail").unwrap_or("compact") == "full";
    let mut items = Vec::with_capacity(max_results);
    let mut total = 0_usize;
    for (direction, edges) in [
        (
            "outgoing",
            state.graph().outgoing_at(index).collect::<Vec<_>>(),
        ),
        (
            "incoming",
            state.graph().incoming_at(index).collect::<Vec<_>>(),
        ),
    ] {
        for edge in edges {
            if filter.is_some_and(|value| edge.kind.as_str() != value) {
                continue;
            }
            if total >= offset && items.len() < max_results {
                let other = if direction == "outgoing" {
                    state.graph().node(edge.target.as_str())
                } else {
                    state.graph().node(edge.source.as_str())
                };
                items.push(if full {
                    json!({"direction": direction, "edge": edge, "node": other})
                } else {
                    json!({
                        "direction": direction,
                        "relation": edge.kind,
                        "provenance": edge.provenance,
                        "node": other.map(|node| json!({
                            "id": node.id,
                            "label": node.label,
                            "kind": node.kind,
                            "span": node.span
                        }))
                    })
                });
            }
            total = total.saturating_add(1);
        }
    }
    let returned = items.len();
    let end = offset.saturating_add(returned);
    Ok(json!({
        "node": state.node(index)?,
        "neighbors": items,
        "page": {
            "offset": offset,
            "returned": returned,
            "total": total,
            "has_more": end < total,
            "next_cursor": (end < total).then(|| format!("v1:{end}"))
        }
    }))
}

pub fn query(state: &RepositoryState, args: &Value) -> Result<Value, String> {
    let seeds = resolve_seeds(state, args)?;
    let depth = usize::try_from(arg_u64(args, "depth").unwrap_or(3)).unwrap_or(3);
    let max_nodes = usize::try_from(arg_u64(args, "max_nodes").unwrap_or(80)).unwrap_or(80);
    let direction = match arg_str(args, "flow_direction").unwrap_or("both") {
        "forward" => Direction::Outgoing,
        "backward" => Direction::Incoming,
        _ => Direction::Both,
    };
    let dfs = arg_str(args, "mode").unwrap_or("bfs") == "dfs";
    let relations = relation_filter(args);
    let (visited, traversed) = traverse(
        state,
        seeds,
        depth,
        max_nodes,
        direction,
        dfs,
        relations.as_ref(),
    );
    let nodes = visited
        .iter()
        .filter_map(|(index, distance)| {
            let node = state.graph().node_at(*index)?;
            crate::operations::node_is_visible(state, index.index(), args)
                .then(|| json!({"node": node, "distance": distance}))
        })
        .collect::<Vec<_>>();
    let edges = traversed
        .into_iter()
        .filter_map(|index| state.graph().edge_at(index))
        .collect::<Vec<_>>();
    Ok(json!({"nodes": nodes, "edges": edges, "truncated": nodes.len() == max_nodes}))
}

pub fn hubs(state: &RepositoryState, args: &Value) -> Value {
    let top = usize::try_from(arg_u64(args, "top_n").unwrap_or(10)).unwrap_or(10);
    let mut nodes = state
        .graph()
        .nodes()
        .iter()
        .enumerate()
        .filter(|(_, node)| !matches!(node.kind, NodeKind::Repository | NodeKind::Package))
        .filter(|(slot, _)| crate::operations::node_is_visible(state, *slot, args))
        .map(|(slot, node)| {
            let index = NodeIndex::new(u32::try_from(slot).unwrap_or(u32::MAX));
            let incoming = state.graph().in_degree(index).unwrap_or(0);
            let outgoing = state.graph().out_degree(index).unwrap_or(0);
            (incoming + outgoing, incoming, outgoing, node)
        })
        .collect::<Vec<_>>();
    nodes.sort_unstable_by(|left, right| {
        right
            .0
            .cmp(&left.0)
            .then_with(|| left.3.id.cmp(&right.3.id))
    });
    json!({
        "hubs": nodes.into_iter().take(top).map(|(degree, incoming, outgoing, node)| {
            json!({"node": node, "degree": degree, "incoming": incoming, "outgoing": outgoing})
        }).collect::<Vec<_>>()
    })
}

pub fn path(state: &RepositoryState, args: &Value) -> Result<Value, String> {
    let source = state.resolve_node(arg_str(args, "source")?)?;
    let target = state.resolve_node(arg_str(args, "target")?)?;
    let mut indices = shortest_path(state.graph(), source, target).unwrap_or_default();
    let max_hops = usize::try_from(arg_u64(args, "max_hops").unwrap_or(8)).unwrap_or(8);
    let bounded_out = indices.len().saturating_sub(1) > max_hops;
    if bounded_out {
        indices.clear();
    }
    let nodes = indices
        .iter()
        .filter_map(|index| state.graph().node_at(*index))
        .collect::<Vec<_>>();
    Ok(json!({
        "found": !nodes.is_empty(),
        "bounded_out": bounded_out,
        "max_hops": max_hops,
        "hops": nodes.len().saturating_sub(1),
        "nodes": nodes
    }))
}

/// Relations that mean "this code depends on that code". Containment is
/// structural, not a dependency, so a reverse dependency walk excludes it.
fn coupling_relations() -> std::collections::BTreeSet<String> {
    [
        "calls",
        "imports",
        "inherits",
        "implements",
        "re_exports",
        "references",
    ]
    .into_iter()
    .map(str::to_owned)
    .collect()
}

pub fn dependents(state: &RepositoryState, args: &Value) -> Result<Value, String> {
    crate::operations::require_graph_precision(args)?;
    let seed = state.resolve_node(arg_str(args, "label")?)?;
    let depth = usize::try_from(arg_u64(args, "depth").unwrap_or(3)).unwrap_or(3);
    let max = usize::try_from(arg_u64(args, "max_nodes").unwrap_or(40)).unwrap_or(40);
    let (visited, _) = traverse(
        state,
        vec![seed],
        depth,
        max + 1,
        Direction::Incoming,
        false,
        Some(&coupling_relations()),
    );
    // Remove the seed by identity: the traversal order is not sorted, so
    // dropping the first entry would silently discard a real dependent.
    let nodes = visited
        .into_iter()
        .filter(|(index, _)| *index != seed)
        .filter_map(|(index, distance)| {
            let node = state.graph().node_at(index)?;
            Some(json!({"node": node, "distance": distance}))
        })
        .take(max)
        .collect::<Vec<_>>();
    Ok(json!({
        "seed": state.node(seed)?,
        "dependents": nodes,
        "relations": coupling_relations().iter().collect::<Vec<_>>(),
        "precision": "graph",
        "semantic_precision": "BOUNDED_STATIC"
    }))
}

fn relation_filter(args: &Value) -> Option<std::collections::BTreeSet<String>> {
    let value = args.get("relation_filter")?;
    if let Some(value) = value.as_str() {
        return Some(std::collections::BTreeSet::from([value.to_owned()]));
    }
    Some(
        value
            .as_array()?
            .iter()
            .filter_map(Value::as_str)
            .map(str::to_owned)
            .collect(),
    )
}
