use super::bounds::{FrontierHop, StopReason, WalkResult};
use crate::engine::RepositoryState;
use std::collections::{BTreeSet, VecDeque};
use weavatrix_graph::{Direction, EdgeIndex, GraphView, NodeIndex};

/// Structural hubs contain or are imported by half the repository, so walking
/// through them turns any local question into "the whole repository". They
/// stay reachable as endpoints but never conduct a traversal.
fn is_hub(state: &RepositoryState, index: NodeIndex) -> bool {
    state.graph().node_at(index).is_some_and(|node| {
        matches!(
            node.kind,
            weavatrix_graph::NodeKind::Repository | weavatrix_graph::NodeKind::Package
        )
    })
}

pub(in crate::operations) fn traverse(
    state: &RepositoryState,
    seeds: Vec<NodeIndex>,
    max_depth: usize,
    max_nodes: usize,
    direction: Direction,
    dfs: bool,
    relations: Option<&BTreeSet<String>>,
) -> WalkResult {
    let max_work = max_nodes.saturating_mul(32).max(64);
    walk(
        state, seeds, max_depth, max_nodes, max_work, direction, dfs, relations,
    )
}

#[allow(clippy::too_many_arguments)]
fn walk(
    state: &RepositoryState,
    seeds: Vec<NodeIndex>,
    max_depth: usize,
    max_nodes: usize,
    max_work: usize,
    direction: Direction,
    dfs: bool,
    relations: Option<&BTreeSet<String>>,
) -> WalkResult {
    let mut seen = BTreeSet::new();
    let mut order = Vec::new();
    let mut edges = BTreeSet::new();
    let mut frontier = Vec::new();
    let mut stop = StopReason::Complete;
    let mut work = 0_usize;
    let mut queue = VecDeque::new();
    for seed in seeds {
        if seen.len() >= max_nodes {
            stop = StopReason::MaxNodes;
            break;
        }
        if seen.insert(seed) {
            order.push((seed, 0));
            queue.push_back((seed, 0));
        }
    }
    while let Some((node, depth)) = pop(&mut queue, dfs) {
        if work >= max_work {
            stop = StopReason::MaxWork;
            break;
        }
        if depth >= max_depth || (depth > 0 && is_hub(state, node)) {
            if depth >= max_depth && has_kept_edge(state, node, direction, relations) {
                stop = prefer(stop, StopReason::MaxDepth);
            }
            continue;
        }
        expand(
            state,
            node,
            depth,
            direction,
            relations,
            max_nodes,
            max_work,
            &mut seen,
            &mut order,
            &mut edges,
            &mut frontier,
            &mut queue,
            &mut work,
            &mut stop,
        );
    }
    if !frontier.is_empty() && stop == StopReason::Complete {
        stop = StopReason::MaxNodes;
    }
    WalkResult {
        nodes: order,
        edges,
        frontier,
        stop,
    }
}

fn pop(queue: &mut VecDeque<(NodeIndex, usize)>, dfs: bool) -> Option<(NodeIndex, usize)> {
    if dfs {
        queue.pop_back()
    } else {
        queue.pop_front()
    }
}

fn prefer(current: StopReason, next: StopReason) -> StopReason {
    if current == StopReason::Complete {
        next
    } else {
        current
    }
}

fn has_kept_edge(
    state: &RepositoryState,
    node: NodeIndex,
    direction: Direction,
    relations: Option<&BTreeSet<String>>,
) -> bool {
    adjacent(state, node, direction).any(|edge| keep(state, edge, relations))
}

pub(super) fn keep(
    state: &RepositoryState,
    edge: EdgeIndex,
    relations: Option<&BTreeSet<String>>,
) -> bool {
    relations.is_none_or(|allowed| {
        state
            .graph()
            .edge_at(edge)
            .is_some_and(|edge| allowed.contains(edge.kind.as_str()))
    })
}

pub(super) fn adjacent(
    state: &RepositoryState,
    node: NodeIndex,
    direction: Direction,
) -> impl Iterator<Item = EdgeIndex> + '_ {
    let outgoing = state.graph().outgoing_edges(node);
    let incoming = state.graph().incoming_edges(node);
    match direction {
        Direction::Outgoing => outgoing.collect::<Vec<_>>().into_iter(),
        Direction::Incoming => incoming.collect::<Vec<_>>().into_iter(),
        Direction::Both => outgoing.chain(incoming).collect::<Vec<_>>().into_iter(),
    }
}

pub(super) fn neighbor(
    state: &RepositoryState,
    node: NodeIndex,
    edge: EdgeIndex,
) -> Option<NodeIndex> {
    let endpoints = state.graph().edge_endpoints(edge)?;
    Some(if endpoints.source() == node {
        endpoints.target()
    } else {
        endpoints.source()
    })
}

#[allow(clippy::too_many_arguments)]
fn expand(
    state: &RepositoryState,
    node: NodeIndex,
    depth: usize,
    direction: Direction,
    relations: Option<&BTreeSet<String>>,
    max_nodes: usize,
    max_work: usize,
    seen: &mut BTreeSet<NodeIndex>,
    order: &mut Vec<(NodeIndex, usize)>,
    edges: &mut BTreeSet<EdgeIndex>,
    frontier: &mut Vec<FrontierHop>,
    queue: &mut VecDeque<(NodeIndex, usize)>,
    work: &mut usize,
    stop: &mut StopReason,
) {
    for edge in adjacent(state, node, direction) {
        if *work >= max_work {
            *stop = prefer(*stop, StopReason::MaxWork);
            return;
        }
        *work += 1;
        if !keep(state, edge, relations) {
            continue;
        }
        let Some(next) = neighbor(state, node, edge) else {
            continue;
        };
        if seen.contains(&next) {
            edges.insert(edge);
            continue;
        }
        if seen.len() >= max_nodes {
            frontier.push(FrontierHop {
                from: node,
                edge,
                to: next,
            });
            *stop = prefer(*stop, StopReason::MaxNodes);
            continue;
        }
        seen.insert(next);
        order.push((next, depth + 1));
        queue.push_back((next, depth + 1));
        edges.insert(edge);
    }
}
