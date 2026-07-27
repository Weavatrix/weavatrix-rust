use crate::RepositoryState;
use crate::tools::graph_walk::traverse;
use crate::tools::{arg_str, arg_u64};
use serde_json::{Value, json};
use std::collections::BTreeSet;
use weavatrix_graph::{Direction, NodeKind};

pub fn endpoint(state: &RepositoryState, args: &Value) -> Result<Value, String> {
    let path = arg_str(args, "path")?;
    let method = arg_str(args, "method").ok();
    let endpoint = state
        .graph()
        .nodes()
        .iter()
        .find(|node| {
            node.kind == NodeKind::Endpoint
                && node.label.ends_with(path)
                && method.is_none_or(|method| node.label.starts_with(method))
        })
        .ok_or_else(|| format!("endpoint not found: {path}"))?;
    let seed = state.resolve_node(endpoint.id.as_str())?;
    let depth = usize::try_from(arg_u64(args, "max_depth").unwrap_or(4)).unwrap_or(4);
    let max_nodes = usize::try_from(arg_u64(args, "max_nodes").unwrap_or(80)).unwrap_or(80);
    let (visited, traversed) = traverse(
        state,
        vec![seed],
        depth,
        max_nodes,
        Direction::Both,
        false,
        None,
    );
    let nodes = visited
        .iter()
        .filter_map(|index| state.graph().node_at(*index))
        .collect::<Vec<_>>();
    let edges = traversed
        .into_iter()
        .filter_map(|index| state.graph().edge_at(index))
        .collect::<Vec<_>>();
    let excerpts = source_excerpts(state, args, &nodes);
    Ok(json!({
        "endpoint": endpoint,
        "nodes": nodes,
        "edges": edges,
        "source_excerpts": excerpts,
        "precision": "bounded_static",
        "dynamic_dispatch": "UNKNOWN"
    }))
}

fn source_excerpts(
    state: &RepositoryState,
    args: &Value,
    nodes: &[&weavatrix_graph::Node],
) -> Vec<Value> {
    let max = usize::try_from(arg_u64(args, "max_excerpts").unwrap_or(8)).unwrap_or(8);
    let context = arg_u64(args, "context_lines").unwrap_or(4);
    let mut files = BTreeSet::new();
    nodes
        .iter()
        .filter(|node| {
            node.span
                .as_ref()
                .is_some_and(|span| files.insert(span.file.clone()))
        })
        .filter_map(|node| {
            super::source::read_source(
                state,
                &json!({"label": node.id.as_str(), "before": context, "after": context}),
            )
            .ok()
        })
        .take(max)
        .collect()
}
