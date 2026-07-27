use crate::RepositoryState;
use crate::tools::arg_str;
use serde_json::Value;
use std::collections::{BTreeSet, VecDeque};
use weavatrix_graph::{Direction, EdgeIndex, GraphView, NodeIndex};

pub(super) fn resolve_seeds(
    state: &RepositoryState,
    args: &Value,
) -> Result<Vec<NodeIndex>, String> {
    let mut seeds = Vec::new();
    for key in ["seed_symbols", "seed_files"] {
        if let Some(values) = args.get(key).and_then(Value::as_array) {
            for value in values.iter().filter_map(Value::as_str) {
                seeds.push(state.resolve_node(value)?);
            }
        }
    }
    if seeds.is_empty() {
        let question = arg_str(args, "question")?.to_ascii_lowercase();
        seeds.extend(
            state
                .graph()
                .nodes()
                .iter()
                .enumerate()
                .filter(|(_, node)| node.label.to_ascii_lowercase().contains(&question))
                .take(12)
                .map(|(index, _)| NodeIndex::new(u32::try_from(index).unwrap_or(u32::MAX))),
        );
    }
    if seeds.is_empty() {
        Err("query did not resolve any graph seed".to_owned())
    } else {
        Ok(seeds)
    }
}

pub(super) fn traverse(
    state: &RepositoryState,
    seeds: Vec<NodeIndex>,
    max_depth: usize,
    max_nodes: usize,
    direction: Direction,
    dfs: bool,
    relations: Option<&BTreeSet<String>>,
) -> (Vec<NodeIndex>, BTreeSet<EdgeIndex>) {
    let mut seen = BTreeSet::new();
    let mut edges = BTreeSet::new();
    let mut queue = VecDeque::new();
    for seed in seeds {
        if seen.insert(seed) {
            queue.push_back((seed, 0));
        }
    }
    while let Some((node, depth)) = if dfs {
        queue.pop_back()
    } else {
        queue.pop_front()
    } {
        if depth >= max_depth || seen.len() >= max_nodes {
            continue;
        }
        let adjacent = match direction {
            Direction::Outgoing => state.graph().outgoing_edges(node).collect::<Vec<_>>(),
            Direction::Incoming => state.graph().incoming_edges(node).collect::<Vec<_>>(),
            Direction::Both => state
                .graph()
                .outgoing_edges(node)
                .chain(state.graph().incoming_edges(node))
                .collect::<Vec<_>>(),
        };
        for edge in adjacent {
            if relations.is_some_and(|allowed| {
                state
                    .graph()
                    .edge_at(edge)
                    .is_none_or(|edge| !allowed.contains(edge.kind.as_str()))
            }) {
                continue;
            }
            let Some(endpoints) = state.graph().edge_endpoints(edge) else {
                continue;
            };
            edges.insert(edge);
            let neighbor = if endpoints.source() == node {
                endpoints.target()
            } else {
                endpoints.source()
            };
            if seen.len() < max_nodes && seen.insert(neighbor) {
                queue.push_back((neighbor, depth + 1));
            }
        }
    }
    (seen.into_iter().collect(), edges)
}
