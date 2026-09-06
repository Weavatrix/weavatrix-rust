use super::lookup::is_structural;
use crate::engine::RepositoryState;
use crate::operations::arg_u64;
use blazingly_json::{Value, json};
use std::collections::{BTreeSet, VecDeque};
use weavatrix_graph::{EdgeIndex, EdgeKind, GraphView, NodeIndex, NodeKind};

pub(super) fn execution_path(
    state: &RepositoryState,
    endpoint: NodeIndex,
    handlers: &[NodeIndex],
    max_depth: usize,
    max_nodes: usize,
) -> (Vec<NodeIndex>, BTreeSet<EdgeIndex>, bool, usize) {
    let mut ordered = Vec::new();
    let mut seen = BTreeSet::new();
    let mut edges = BTreeSet::new();
    let mut omitted = 0_usize;
    let mut call_seeds = Vec::new();

    for index in std::iter::once(endpoint).chain(handlers.iter().copied()) {
        push_node(&mut ordered, &mut seen, &mut omitted, max_nodes, index);
    }
    retain_expose_edges(state, endpoint, handlers, &mut edges);

    for handler in handlers {
        expand_handler_seeds(&mut HandlerSeedExpand {
            state,
            handler: *handler,
            ordered: &mut ordered,
            seen: &mut seen,
            edges: &mut edges,
            omitted: &mut omitted,
            max_nodes,
            call_seeds: &mut call_seeds,
        });
    }
    if ordered.len() > max_nodes {
        omitted += ordered.len() - max_nodes;
        ordered.truncate(max_nodes);
        seen = ordered.iter().copied().collect();
        call_seeds.retain(|index| seen.contains(index));
        edges.retain(|edge| edge_touches_seen(state, *edge, &seen));
    }

    let mut queue = VecDeque::new();
    for seed in call_seeds.into_iter().filter(|index| seen.contains(index)) {
        queue.push_back((seed, 0_usize));
    }
    while let Some((node, depth)) = queue.pop_front() {
        if depth >= max_depth {
            continue;
        }
        for edge in state.graph().outgoing_edges(node) {
            let Some(item) = state.graph().edge_at(edge) else {
                continue;
            };
            if item.kind != EdgeKind::Calls {
                continue;
            }
            let Some(target) = state.graph().node_index(item.target.as_str()) else {
                continue;
            };
            let Some(target_node) = state.graph().node_at(target) else {
                continue;
            };
            if is_structural(&target_node.kind) {
                continue;
            }
            edges.insert(edge);
            if !seen.insert(target) {
                continue;
            }
            if ordered.len() >= max_nodes {
                seen.remove(&target);
                omitted += 1;
                continue;
            }
            ordered.push(target);
            queue.push_back((target, depth + 1));
        }
    }

    (ordered, edges, omitted > 0, omitted)
}

struct HandlerSeedExpand<'a> {
    state: &'a RepositoryState,
    handler: NodeIndex,
    ordered: &'a mut Vec<NodeIndex>,
    seen: &'a mut BTreeSet<NodeIndex>,
    edges: &'a mut BTreeSet<EdgeIndex>,
    omitted: &'a mut usize,
    max_nodes: usize,
    call_seeds: &'a mut Vec<NodeIndex>,
}

fn expand_handler_seeds(ctx: &mut HandlerSeedExpand<'_>) {
    let Some(node) = ctx.state.graph().node_at(ctx.handler) else {
        return;
    };
    if matches!(node.kind, NodeKind::Function | NodeKind::Method) {
        ctx.call_seeds.push(ctx.handler);
        return;
    }
    if node.kind != NodeKind::File {
        return;
    }
    // Route files expose endpoints without a function owner. Prefer imported
    // modules and their functions over Contains children, so a sibling GET in
    // the same file cannot displace middleware/handler under a node cap.
    for edge in ctx.state.graph().outgoing_edges(ctx.handler) {
        let Some(item) = ctx.state.graph().edge_at(edge) else {
            continue;
        };
        if item.kind != EdgeKind::Imports {
            continue;
        }
        let Some(target) = ctx.state.graph().node_index(item.target.as_str()) else {
            continue;
        };
        let Some(target_node) = ctx.state.graph().node_at(target) else {
            continue;
        };
        if is_structural(&target_node.kind) {
            continue;
        }
        ctx.edges.insert(edge);
        if push_node(ctx.ordered, ctx.seen, ctx.omitted, ctx.max_nodes, target) {
            for nested in ctx.state.graph().outgoing_edges(target) {
                let Some(nested_edge) = ctx.state.graph().edge_at(nested) else {
                    continue;
                };
                if nested_edge.kind != EdgeKind::Contains {
                    continue;
                }
                let Some(symbol) = ctx.state.graph().node_index(nested_edge.target.as_str()) else {
                    continue;
                };
                let Some(symbol_node) = ctx.state.graph().node_at(symbol) else {
                    continue;
                };
                if !matches!(symbol_node.kind, NodeKind::Function | NodeKind::Method) {
                    continue;
                }
                ctx.edges.insert(nested);
                if push_node(ctx.ordered, ctx.seen, ctx.omitted, ctx.max_nodes, symbol) {
                    ctx.call_seeds.push(symbol);
                }
            }
        }
    }
}

fn retain_expose_edges(
    state: &RepositoryState,
    endpoint: NodeIndex,
    handlers: &[NodeIndex],
    edges: &mut BTreeSet<EdgeIndex>,
) {
    for edge in state.graph().incoming_edges(endpoint) {
        let Some(item) = state.graph().edge_at(edge) else {
            continue;
        };
        if item.kind == EdgeKind::Exposes
            && state
                .graph()
                .node_index(item.source.as_str())
                .is_some_and(|source| handlers.contains(&source))
        {
            edges.insert(edge);
        }
    }
}

fn push_node(
    ordered: &mut Vec<NodeIndex>,
    seen: &mut BTreeSet<NodeIndex>,
    omitted: &mut usize,
    max_nodes: usize,
    index: NodeIndex,
) -> bool {
    if !seen.insert(index) {
        return true;
    }
    if ordered.len() >= max_nodes {
        seen.remove(&index);
        *omitted += 1;
        return false;
    }
    ordered.push(index);
    true
}

fn edge_touches_seen(state: &RepositoryState, edge: EdgeIndex, seen: &BTreeSet<NodeIndex>) -> bool {
    state.graph().edge_at(edge).is_some_and(|item| {
        state
            .graph()
            .node_index(item.source.as_str())
            .is_some_and(|source| seen.contains(&source))
    })
}

pub(super) fn source_excerpts(
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
            crate::operations::source::read_source(
                state,
                &json!({"label": node.id.as_str(), "before": context, "after": context}),
            )
            .ok()
        })
        .take(max)
        .collect()
}
