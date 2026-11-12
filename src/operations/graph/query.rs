use super::relation_filter;
use super::seeds::resolve_seeds;
use super::walk::traverse;
use crate::engine::{LabelMatch, RepositoryState};
use crate::operations::{arg_str, arg_u64, optional_str};
use blazingly_json::{Value, json};
use std::collections::BTreeSet;
use weavatrix_graph::{Direction, Node, NodeIndex, NodeKind};

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
                .then(|| json!({"node": super::compact_node(node), "distance": distance}))
        })
        .collect::<Vec<_>>();
    let edges = walk
        .edges
        .iter()
        .filter_map(|index| state.graph().edge_at(*index).map(super::compact_edge))
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
    let seeds = dependent_seeds(state, args)?;
    let depth = usize::try_from(arg_u64(args, "depth").unwrap_or(3)).unwrap_or(3);
    let max = usize::try_from(arg_u64(args, "max_nodes").unwrap_or(40)).unwrap_or(40);
    let walk = traverse(
        state,
        seeds.clone(),
        depth,
        max.saturating_add(1),
        Direction::Incoming,
        false,
        Some(&super::coupling::coupling_relations()),
    );
    let seed_slots = seeds
        .iter()
        .map(|index| index.index())
        .collect::<BTreeSet<_>>();
    let nodes = walk
        .nodes
        .iter()
        .filter(|(index, _)| !seed_slots.contains(&index.index()))
        .filter_map(|(index, distance)| {
            let node = state.graph().node_at(*index)?;
            Some(json!({"node": super::compact_node(node), "distance": distance}))
        })
        .take(max)
        .collect::<Vec<_>>();
    let primary = primary_seed(state, &seeds);
    let mut report = json!({
        "seed": super::compact_node(state.node(primary)?),
        "dependents": nodes,
        "relations": super::coupling::coupling_relations().iter().collect::<Vec<_>>(),
        "precision": "graph",
        "semantic_precision": "BOUNDED_STATIC",
        "truncated": !walk.evidence_complete()
    });
    if seeds.len() > 1
        && let Some(object) = report.as_object_mut()
    {
        let listed = seeds
            .iter()
            .filter_map(|index| state.node(*index).ok().map(super::compact_node))
            .collect::<Vec<_>>();
        object.insert("seeds".to_owned(), json!(listed));
    }
    if let (Some(object), Some(extra)) = (report.as_object_mut(), walk.report().as_object_mut()) {
        object.append(extra);
    }
    Ok(report)
}

fn dependent_seeds(state: &RepositoryState, args: &Value) -> Result<Vec<NodeIndex>, String> {
    let label = optional_str(args, "label")?
        .map(str::trim)
        .filter(|value| !value.is_empty());
    let path = optional_str(args, "path")?
        .map(str::trim)
        .filter(|value| !value.is_empty());
    match (label, path) {
        (None, None) => Err("get_dependents requires label or path".to_owned()),
        (None, Some(path)) => file_seeds(state, path),
        (Some(label), path) => seeds_for_label(state, label, path),
    }
}

fn seeds_for_label(
    state: &RepositoryState,
    label: &str,
    path: Option<&str>,
) -> Result<Vec<NodeIndex>, String> {
    let picked = match state.match_label(label)? {
        LabelMatch::One(index) => vec![index],
        LabelMatch::None => return Err(format!("node not found: {label}")),
        LabelMatch::Many(indexes) => {
            let Some(path) = path else {
                return Err(state.resolve_node(label).unwrap_err());
            };
            indexes
                .into_iter()
                .filter(|index| {
                    state
                        .node(*index)
                        .is_ok_and(|node| node_in_file(node, path))
                })
                .collect()
        }
    };
    if let Some(path) = path {
        let in_file = picked
            .into_iter()
            .filter(|index| {
                state
                    .node(*index)
                    .is_ok_and(|node| node_in_file(node, path))
            })
            .collect::<Vec<_>>();
        return match in_file.len() {
            0 => Err(format!("node {label} is not in {path}")),
            1 => Ok(in_file),
            _ => Err(format!(
                "ambiguous node label {label:?} in {path}; pass a candidate id"
            )),
        };
    }
    match picked.len() {
        1 => Ok(picked),
        0 => Err(format!("node not found: {label}")),
        _ => Err(state.resolve_node(label).unwrap_err()),
    }
}

fn file_seeds(state: &RepositoryState, path: &str) -> Result<Vec<NodeIndex>, String> {
    let mut seeds = Vec::new();
    for (slot, node) in state.graph().nodes().iter().enumerate() {
        if !node_in_file(node, path) {
            continue;
        }
        seeds.push(NodeIndex::new(
            u32::try_from(slot).map_err(|_| "node index overflow")?,
        ));
    }
    if seeds.is_empty() {
        return Err(format!("no graph nodes for file {path}"));
    }
    Ok(seeds)
}

fn primary_seed(state: &RepositoryState, seeds: &[NodeIndex]) -> NodeIndex {
    seeds
        .iter()
        .copied()
        .find(|index| {
            state
                .node(*index)
                .is_ok_and(|node| node.kind == NodeKind::File)
        })
        .unwrap_or(seeds[0])
}

fn node_in_file(node: &Node, path: &str) -> bool {
    let wanted = file_key(path);
    if node.kind == NodeKind::File && file_key(&node.label) == wanted {
        return true;
    }
    node.span
        .as_ref()
        .is_some_and(|span| file_key(&span.file) == wanted)
}

fn file_key(path: &str) -> String {
    path.replace('\\', "/")
        .trim_start_matches("./")
        .trim_end_matches('/')
        .to_ascii_lowercase()
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
