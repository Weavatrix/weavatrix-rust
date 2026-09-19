mod impact;
mod view;

use crate::engine::RepositoryState;
use crate::operations::domain_walk;
use crate::operations::{arg_str, optional_str, optional_u64, reject_unknown_arguments};
use blazingly_json::{Value, json};
use weavatrix_graph::Node;

pub(super) fn inventory(state: &RepositoryState, args: &Value) -> Result<Value, String> {
    reject_unknown_arguments("web3_inventory", args, &["path", "max_results"])?;
    let path = optional_str(args, "path")?;
    let max = bounded(optional_u64(args, "max_results")?.unwrap_or(200), 500)?;
    let (artifacts, art_found) = take_nodes(state, "web3.artifact", path, max);
    let (events, event_found) = take_nodes(state, "web3.abi.event", path, max);
    let (functions, fn_found) = take_nodes(state, "web3.abi.function", path, max);
    let mut members = events;
    members.extend(functions);
    let member_found = event_found + fn_found;
    let member_cut = members.len() > max;
    members.truncate(max);
    let (consumers, consumer_found) = take_nodes(state, "web3.consumer", path, max);
    let found = art_found + member_found + consumer_found;
    let shown = artifacts.len() + members.len() + consumers.len();
    let truncated = art_found > artifacts.len() || member_cut || consumer_found > consumers.len();
    Ok(json!({
        "artifacts": artifacts,
        "members": members,
        "consumers": consumers,
        "coverage": view::coverage(state),
        "bounds": {
            "truncated": truncated,
            "found": found,
            "shown": shown,
            "reasons": if truncated { vec!["max_results"] } else { Vec::<&str>::new() },
            "revision": state.snapshot().revision,
            "runtime": false
        }
    }))
}

pub(super) fn trace(state: &RepositoryState, args: &Value) -> Result<Value, String> {
    reject_unknown_arguments(
        "web3_trace",
        args,
        &["label", "depth", "max_nodes", "cursor"],
    )?;
    let label = arg_str(args, "label")?;
    let depth = usize::try_from(optional_u64(args, "depth")?.unwrap_or(1))
        .map_err(|_| "depth is too large")?;
    if depth == 0 || depth > 32 {
        return Err("depth must be between 1 and 32".to_owned());
    }
    let max = bounded(optional_u64(args, "max_nodes")?.unwrap_or(64), 200)?;
    let offset = domain_walk::page_offset_for(args, &state.snapshot().revision)?;
    let index = state.resolve_node(label)?;
    let node = state.node(index)?;
    let (steps, depth_cut) = walk_steps(state, node, depth);
    let total = steps.len();
    let end = offset.saturating_add(max).min(total);
    let page = if offset > total {
        Vec::new()
    } else {
        steps[offset..end].to_vec()
    };
    let mut reasons = Vec::new();
    if depth_cut {
        reasons.push("depth");
    }
    if end < total {
        reasons.push("page");
    }
    Ok(json!({
        "selected": { "id": node.id, "label": node.label, "kind": node.kind, "span": node.span },
        "steps": page,
        "page": {
            "offset": offset,
            "returned": page.len(),
            "total": total,
            "has_more": end < total,
            "next_cursor": (end < total).then(|| format!("v1:{end}:{}", state.snapshot().revision))
        },
        "coverage": view::coverage(state),
        "bounds": {
            "truncated": !reasons.is_empty(),
            "found": total,
            "shown": page.len(),
            "reasons": reasons,
            "depth": depth,
            "max_nodes": max,
            "revision": state.snapshot().revision,
            "runtime": false
        }
    }))
}

pub(super) fn impact(state: &RepositoryState, args: &Value) -> Result<Value, String> {
    impact::impact(state, args)
}

pub(super) fn context(state: &RepositoryState, args: &Value) -> Result<Value, String> {
    reject_unknown_arguments("web3_context", args, &["label", "task", "max_related"])?;
    let label = arg_str(args, "label")?;
    let task = optional_str(args, "task")?.unwrap_or("change-event");
    let max = bounded(optional_u64(args, "max_related")?.unwrap_or(24), 200)?;
    let index = state.resolve_node(label)?;
    let node = state.node(index)?;
    let domains = view::domain_names(state, node.id.as_str());
    let all_consumers = view::consumers_of(state, node.id.as_str());
    let consumer_found = all_consumers.len();
    let consumers = all_consumers.into_iter().take(max).collect::<Vec<_>>();
    let domains_all = domains.len();
    let domain_cut = domains_all > max;
    let mut gaps = vec![
        "deployment state is not_provided".to_owned(),
        "historical log coverage is not_provided".to_owned(),
    ];
    if domains.iter().any(|name| name.contains("unresolved")) {
        gaps.push("consumer ABI target is unresolved".to_owned());
    }
    if task.contains("event") {
        gaps.push("decoder profile was not executed against a live log".to_owned());
    }
    Ok(json!({
        "selected": { "id": node.id, "label": node.label, "kind": node.kind, "span": node.span },
        "task": task,
        "domains": domains.into_iter().take(max).collect::<Vec<_>>(),
        "consumers": consumers,
        "fragments": fragments(state, node),
        "gaps": gaps,
        "coverage": view::coverage(state),
        "bounds": {
            "truncated": domain_cut || consumer_found > consumers.len(),
            "found": domains_all + consumer_found,
            "shown": domains_all.min(max) + consumers.len(),
            "reasons": if domain_cut || consumer_found > max { vec!["max_related"] } else { Vec::<&str>::new() },
            "revision": state.snapshot().revision,
            "runtime": false
        }
    }))
}

fn walk_steps(state: &RepositoryState, start: &Node, depth: usize) -> (Vec<Value>, bool) {
    let mut steps = vec![json!({
        "id": start.id,
        "label": start.label,
        "kind": start.kind,
        "span": start.span,
        "hop": 0
    })];
    let mut seen = std::collections::BTreeSet::from([start.id.as_str().to_owned()]);
    let mut frontier = vec![(start.id.as_str().to_owned(), 0_usize)];
    let mut depth_cut = false;
    while let Some((id, hop)) = frontier.pop() {
        if hop >= depth {
            continue;
        }
        for edge in state.graph().edges() {
            if edge.source.as_str() != id && edge.target.as_str() != id {
                continue;
            }
            let other = if edge.source.as_str() == id {
                edge.target.as_str()
            } else {
                edge.source.as_str()
            };
            if !seen.insert(other.to_owned()) {
                continue;
            }
            if hop + 1 == depth {
                depth_cut |= state.graph().edges().iter().any(|next| {
                    (next.source.as_str() == other || next.target.as_str() == other)
                        && next.source.as_str() != id
                        && next.target.as_str() != id
                });
            }
            if let Some(next) = state.graph().node(other) {
                steps.push(json!({
                    "id": next.id,
                    "label": next.label,
                    "kind": next.kind,
                    "relation": edge.kind,
                    "span": next.span,
                    "hop": hop + 1
                }));
                frontier.push((other.to_owned(), hop + 1));
            }
        }
    }
    (steps, depth_cut)
}

fn take_nodes(
    state: &RepositoryState,
    kind: &str,
    path: Option<&str>,
    max: usize,
) -> (Vec<Value>, usize) {
    let all = state
        .graph()
        .nodes()
        .iter()
        .filter(|node| node.kind.as_str() == kind && node.id.as_str().starts_with("symbol:"))
        .filter(|node| path.is_none_or(|filter| view::in_path(node, filter)))
        .map(|node| {
            json!({
                "id": node.id,
                "label": node.label,
                "kind": node.kind,
                "file": view::file_of(node),
                "span": node.span
            })
        })
        .collect::<Vec<_>>();
    let found = all.len();
    (all.into_iter().take(max).collect(), found)
}

fn fragments(state: &RepositoryState, node: &Node) -> Vec<Value> {
    if node.span.is_none() {
        return Vec::new();
    }
    let Ok(source) = crate::operations::source::read_source(
        state,
        &json!({ "label": node.id.as_str(), "before": 1, "after": 8 }),
    ) else {
        return Vec::new();
    };
    vec![json!({
        "path": source["path"],
        "start_line": source["start_line"],
        "lines": source["lines"]
    })]
}

fn bounded(value: u64, max: u64) -> Result<usize, String> {
    if value == 0 || value > max {
        return Err(format!("count must be between 1 and {max}"));
    }
    usize::try_from(value).map_err(|_| "count is too large".to_owned())
}
