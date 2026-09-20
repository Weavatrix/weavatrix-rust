use super::relation_filter;
use super::seeds::resolve_seeds;
use super::walk::traverse;
use crate::engine::RepositoryState;
use crate::operations::{arg_str, arg_u64};
use blazingly_json::{Value, json};
use weavatrix_graph::{Direction, NodeIndex};

pub fn query(state: &RepositoryState, args: &Value) -> Result<Value, String> {
    let resolved = resolve_seeds(state, args)?;
    let depth = usize::try_from(arg_u64(args, "depth").unwrap_or(3)).unwrap_or(3);
    let max_nodes = usize::try_from(arg_u64(args, "max_nodes").unwrap_or(80)).unwrap_or(80);
    let direction = match arg_str(args, "flow_direction") {
        Ok("forward") => Direction::Outgoing,
        Ok("backward") => Direction::Incoming,
        Ok(_) => Direction::Both,
        Err(_) => resolved.direction.unwrap_or(Direction::Both),
    };
    let dfs = arg_str(args, "mode").unwrap_or("bfs") == "dfs";
    let relations = relation_filter(args)?;
    let walk = traverse(
        state,
        resolved.seeds.clone(),
        depth,
        max_nodes,
        direction,
        dfs,
        relations.as_ref(),
    );
    let nodes = walk
        .nodes
        .iter()
        .filter_map(|(index, distance)| {
            let node = state.graph().node_at(*index)?;
            crate::operations::node_is_visible(state, index.index(), args)
                .then(|| json!({"node": node, "distance": distance}))
        })
        .collect::<Vec<_>>();
    let edges = walk
        .edges
        .iter()
        .filter_map(|index| state.graph().edge_at(*index))
        .collect::<Vec<_>>();
    let budget = crate::operations::token_budget::requested(args)?;
    let mut bounds = walk.report();
    let mut report = json!({
        "nodes": nodes,
        "edges": edges,
        "truncated": !walk.evidence_complete(),
        "resolved_seeds": seed_summaries(state, &resolved.seeds),
        "alternatives": seed_summaries(state, &resolved.alternatives),
        "intent": resolved.intent,
        "tokens": resolved.tokens
    });
    if let (Some(object), Some(extra)) = (report.as_object_mut(), bounds.as_object_mut()) {
        object.append(extra);
    }
    crate::operations::token_budget::fit(&mut report, budget, &["/edges", "/nodes"]);
    let dropped = report["token_budget"]["dropped_items"]
        .as_u64()
        .unwrap_or(0);
    if dropped > 0
        && let Some(value) = report.pointer_mut("/truncated")
    {
        *value = json!(true);
    }
    Ok(report)
}

pub fn dependents(state: &RepositoryState, args: &Value) -> Result<Value, String> {
    crate::operations::require_graph_precision(args)?;
    let seed = state.resolve_node(arg_str(args, "label")?)?;
    let depth = usize::try_from(arg_u64(args, "depth").unwrap_or(3)).unwrap_or(3);
    let max = usize::try_from(arg_u64(args, "max_nodes").unwrap_or(40)).unwrap_or(40);
    let walk = traverse(
        state,
        vec![seed],
        depth,
        max.saturating_add(1),
        Direction::Incoming,
        false,
        Some(&super::coupling::coupling_relations()),
    );
    let nodes = walk
        .nodes
        .iter()
        .filter(|(index, _)| *index != seed)
        .filter_map(|(index, distance)| {
            let node = state.graph().node_at(*index)?;
            Some(json!({"node": node, "distance": distance}))
        })
        .take(max)
        .collect::<Vec<_>>();
    let mut report = json!({
        "seed": state.node(seed)?,
        "dependents": nodes,
        "relations": super::coupling::coupling_relations().iter().collect::<Vec<_>>(),
        "precision": "graph",
        "semantic_precision": "BOUNDED_STATIC",
        "truncated": !walk.evidence_complete()
    });
    if let (Some(object), Some(extra)) = (report.as_object_mut(), walk.report().as_object_mut()) {
        object.append(extra);
    }
    Ok(report)
}

fn seed_summaries(state: &RepositoryState, seeds: &[NodeIndex]) -> Vec<Value> {
    seeds
        .iter()
        .filter_map(|index| {
            let node = state.graph().node_at(*index)?;
            Some(json!({
                "id": node.id,
                "label": node.label,
                "kind": node.kind,
                "span": node.span
            }))
        })
        .collect()
}
