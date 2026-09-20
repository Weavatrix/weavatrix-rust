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

pub fn path(state: &RepositoryState, args: &Value) -> Result<Value, String> {
    let source = state.resolve_node(arg_str(args, "source")?)?;
    let target = state.resolve_node(arg_str(args, "target")?)?;
    let max_hops = usize::try_from(arg_u64(args, "max_hops").unwrap_or(8)).unwrap_or(8);
    let relations = relation_filter(args)?;
    let direction = match arg_str(args, "flow_direction").unwrap_or("forward") {
        "backward" => Direction::Incoming,
        "both" => Direction::Both,
        _ => Direction::Outgoing,
    };
    let (found, bounded_out) = bounded_path(
        state,
        source,
        target,
        direction,
        max_hops,
        relations.as_ref(),
    );
    let hops_json = path_hops(state, &found);
    Ok(json!({
        "found": !found.is_empty(),
        "bounded_out": bounded_out,
        "max_hops": max_hops,
        "hops": found.len().saturating_sub(1),
        "nodes": found.iter().filter_map(|index| state.graph().node_at(*index)).collect::<Vec<_>>(),
        "witnesses": hops_json,
        "execution_status": "OK",
        "evidence_completeness": if bounded_out { "INCOMPLETE" } else { "COMPLETE" },
        "stop_reason": if bounded_out { "MAX_HOPS" } else { "COMPLETE" }
    }))
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

fn bounded_path(
    state: &RepositoryState,
    source: NodeIndex,
    target: NodeIndex,
    direction: Direction,
    max_hops: usize,
    relations: Option<&std::collections::BTreeSet<String>>,
) -> (Vec<NodeIndex>, bool) {
    use std::collections::{HashMap, VecDeque};
    let mut seen = HashMap::new();
    let mut queue = VecDeque::new();
    let mut hit_cap = false;
    seen.insert(source, None);
    queue.push_back((source, 0_usize));
    while let Some((node, depth)) = queue.pop_front() {
        if node == target {
            return (rebuild(source, target, &seen), false);
        }
        if depth >= max_hops {
            hit_cap = true;
            continue;
        }
        for edge in super::walk::adjacent(state, node, direction) {
            if !super::walk::keep(state, edge, relations) {
                continue;
            }
            let Some(next) = super::walk::neighbor(state, node, edge) else {
                continue;
            };
            if seen.contains_key(&next) {
                continue;
            }
            seen.insert(next, Some(node));
            queue.push_back((next, depth + 1));
        }
    }
    (Vec::new(), hit_cap)
}

fn rebuild(
    source: NodeIndex,
    target: NodeIndex,
    seen: &std::collections::HashMap<NodeIndex, Option<NodeIndex>>,
) -> Vec<NodeIndex> {
    let mut path = vec![target];
    let mut current = target;
    while current != source {
        let Some(Some(prev)) = seen.get(&current).copied() else {
            return Vec::new();
        };
        path.push(prev);
        current = prev;
    }
    path.reverse();
    path
}

fn path_hops(state: &RepositoryState, nodes: &[NodeIndex]) -> Vec<Value> {
    nodes
        .windows(2)
        .filter_map(|pair| {
            let from = state.graph().node_at(pair[0])?;
            let to = state.graph().node_at(pair[1])?;
            let edge = state
                .graph()
                .outgoing_at(pair[0])
                .find(|edge| edge.target.as_str() == to.id.as_str())
                .or_else(|| {
                    state
                        .graph()
                        .incoming_at(pair[0])
                        .find(|edge| edge.source.as_str() == to.id.as_str())
                })?;
            Some(json!({
                "from": from.id,
                "to": to.id,
                "relation": edge.kind,
                "provenance": edge.provenance
            }))
        })
        .collect()
}
