use std::collections::{BTreeMap, BTreeSet};
use weavatrix_graph::{Graph, NodeIndex};

pub(super) fn strongly_connected_files(
    graph: &Graph,
    adjacency: &BTreeMap<NodeIndex, BTreeSet<NodeIndex>>,
) -> Vec<Vec<String>> {
    strongly_connected_nodes(adjacency)
        .into_iter()
        .map(|component| file_ids(graph, component))
        .collect()
}

pub(super) fn file_ids(
    graph: &Graph,
    component: impl IntoIterator<Item = NodeIndex>,
) -> Vec<String> {
    let mut members = component
        .into_iter()
        .filter_map(|index| graph.node_at(index))
        .map(|node| node.id.to_string())
        .collect::<Vec<_>>();
    members.sort_unstable();
    members
}

/// Deterministic, iterative Kosaraju traversal.
pub(super) fn strongly_connected_nodes(
    adjacency: &BTreeMap<NodeIndex, BTreeSet<NodeIndex>>,
) -> Vec<Vec<NodeIndex>> {
    let mut seen = BTreeSet::new();
    let mut order = Vec::new();
    for start in adjacency.keys().copied() {
        if seen.contains(&start) {
            continue;
        }
        let mut stack = vec![(start, false)];
        while let Some((node, expanded)) = stack.pop() {
            if expanded {
                order.push(node);
                continue;
            }
            if !seen.insert(node) {
                continue;
            }
            stack.push((node, true));
            if let Some(neighbors) = adjacency.get(&node) {
                stack.extend(neighbors.iter().rev().map(|neighbor| (*neighbor, false)));
            }
        }
    }
    let mut reverse = BTreeMap::<NodeIndex, BTreeSet<NodeIndex>>::new();
    for (source, targets) in adjacency {
        reverse.entry(*source).or_default();
        for target in targets {
            reverse.entry(*target).or_default().insert(*source);
        }
    }
    let mut assigned = BTreeSet::new();
    let mut components = Vec::new();
    for start in order.into_iter().rev() {
        if !assigned.insert(start) {
            continue;
        }
        let mut component = Vec::new();
        let mut stack = vec![start];
        while let Some(node) = stack.pop() {
            component.push(node);
            if let Some(neighbors) = reverse.get(&node) {
                for neighbor in neighbors.iter().rev() {
                    if assigned.insert(*neighbor) {
                        stack.push(*neighbor);
                    }
                }
            }
        }
        if component.len() > 1 {
            component.sort_unstable();
            components.push(component);
        }
    }
    components.sort_unstable();
    components
}

pub(super) fn node_index(slot: usize) -> NodeIndex {
    NodeIndex::new(u32::try_from(slot).unwrap_or(u32::MAX))
}
