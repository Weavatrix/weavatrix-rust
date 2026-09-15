use crate::engine::RepositoryState;
use crate::operations::{arg_str, optional_u64, reject_unknown_arguments};
use blazingly_json::{Value, json};
use std::collections::{BTreeSet, VecDeque};
use weavatrix_graph::{Edge, EdgeKind};

const TRACE_PAGE: u64 = 100;

pub(super) fn trace(state: &RepositoryState, args: &Value) -> Result<Value, String> {
    reject_unknown_arguments(
        "n8n_trace",
        args,
        &["label", "depth", "max_nodes", "cursor"],
    )?;
    let label = arg_str(args, "label")?;
    let depth = usize::try_from(optional_u64(args, "depth")?.unwrap_or(8))
        .map_err(|_| "depth is too large")?;
    if !(1..=32).contains(&depth) {
        return Err("depth must be between 1 and 32".to_owned());
    }
    let max_nodes = usize::try_from(optional_u64(args, "max_nodes")?.unwrap_or(TRACE_PAGE))
        .map_err(|_| "max_nodes is too large")?;
    if max_nodes == 0 || max_nodes > 500 {
        return Err("max_nodes must be between 1 and 500".to_owned());
    }
    let offset = crate::operations::graph::page_offset(args)?;
    let start = state.resolve_node(label)?;
    let start_node = state.node(start)?;
    let start_id = start_node.id.clone();
    let mut cycle = false;
    let mut steps = Vec::new();
    walk_relation(
        state,
        start_id.as_str(),
        depth,
        "flows_to",
        &mut steps,
        &mut cycle,
    );
    walk_relation(
        state,
        start_id.as_str(),
        depth,
        "depends_on_output",
        &mut steps,
        &mut cycle,
    );
    walk_relation(
        state,
        start_id.as_str(),
        depth,
        "handles_error_with",
        &mut steps,
        &mut cycle,
    );
    walk_relation(
        state,
        start_id.as_str(),
        depth,
        "calls_workflow",
        &mut steps,
        &mut cycle,
    );
    let total = steps.len();
    let end = offset.saturating_add(max_nodes).min(total);
    let page = if offset > total {
        Vec::new()
    } else {
        steps[offset..end].to_vec()
    };
    Ok(json!({
        "label": label,
        "start": start_id,
        "steps": page,
        "cycle": cycle,
        "page": {
            "offset": offset,
            "returned": page.len(),
            "total": total,
            "has_more": end < total,
            "next_cursor": (end < total).then(|| format!("v1:{end}"))
        },
        "bounds": {"truncated": end < total, "runtime": false}
    }))
}

fn walk_relation(
    state: &RepositoryState,
    start: &str,
    depth: usize,
    relation: &str,
    steps: &mut Vec<Value>,
    cycle: &mut bool,
) {
    let seeds = expand_ports(state, start);
    let mut seen = BTreeSet::from([start.to_owned()]);
    let mut queue = VecDeque::new();
    for seed in seeds {
        seen.insert(seed.clone());
        queue.push_back((seed, 0_usize));
    }
    while let Some((id, hop)) = queue.pop_front() {
        if hop >= depth {
            continue;
        }
        for edge in state.graph().edges() {
            if edge.kind.as_str() != relation {
                continue;
            }
            let Some(next) = natural_next(edge, &id) else {
                continue;
            };
            if !seen.insert(next.to_owned()) {
                *cycle = true;
                continue;
            }
            steps.push(json!({
                "from": id,
                "to": next,
                "relation": edge.kind,
                "detail": edge.provenance.detail,
                "hop": hop + 1
            }));
            for extra in expand_ports(state, next) {
                if seen.insert(extra.clone()) {
                    queue.push_back((extra, hop + 1));
                }
            }
            queue.push_back((next.to_owned(), hop + 1));
        }
    }
}

fn natural_next<'a>(edge: &'a Edge, id: &str) -> Option<&'a str> {
    if edge.source.as_str() == id {
        Some(edge.target.as_str())
    } else {
        None
    }
}

fn expand_ports(state: &RepositoryState, id: &str) -> Vec<String> {
    let mut ids = vec![id.to_owned()];
    for edge in state.graph().edges() {
        if edge.kind != EdgeKind::Contains {
            continue;
        }
        if edge.source.as_str() == id {
            if state
                .graph()
                .node(edge.target.as_str())
                .is_some_and(|node| node.kind.as_str() == "n8n.port")
            {
                ids.push(edge.target.as_str().to_owned());
            }
        } else if edge.target.as_str() == id
            && state
                .graph()
                .node(edge.source.as_str())
                .is_some_and(|node| matches!(node.kind.as_str(), "n8n.node" | "n8n.workflow"))
        {
            ids.push(edge.source.as_str().to_owned());
        }
    }
    ids
}
