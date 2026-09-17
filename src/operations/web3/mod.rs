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
    let artifacts = take_nodes(state, "web3.artifact", path, max);
    let members = take_nodes(state, "web3.abi.event", path, max)
        .into_iter()
        .chain(take_nodes(state, "web3.abi.function", path, max))
        .take(max)
        .collect::<Vec<_>>();
    let consumers = take_nodes(state, "web3.consumer", path, max);
    Ok(json!({
        "artifacts": artifacts,
        "members": members,
        "consumers": consumers,
        "coverage": view::coverage(state),
        "bounds": {
            "truncated": false,
            "found": artifacts.len() + members.len() + consumers.len(),
            "shown": artifacts.len() + members.len() + consumers.len(),
            "reasons": Value::Array(Vec::new()),
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
    let mut steps = vec![json!({
        "id": node.id,
        "label": node.label,
        "kind": node.kind,
        "span": node.span,
        "hop": 0
    })];
    if depth > 0 {
        for edge in state.graph().edges() {
            if edge.source.as_str() != node.id.as_str() && edge.target.as_str() != node.id.as_str()
            {
                continue;
            }
            let other = if edge.source.as_str() == node.id.as_str() {
                edge.target.as_str()
            } else {
                edge.source.as_str()
            };
            if let Some(next) = state.graph().node(other) {
                steps.push(json!({
                    "id": next.id,
                    "label": next.label,
                    "kind": next.kind,
                    "relation": edge.kind,
                    "span": next.span,
                    "hop": 1
                }));
            }
        }
    }
    let total = steps.len();
    let end = offset.saturating_add(max).min(total);
    let page = if offset > total {
        Vec::new()
    } else {
        steps[offset..end].to_vec()
    };
    let mut reasons = Vec::new();
    if depth == 1 && total > 1 {
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
    let consumers = view::consumers_of(state, node.id.as_str())
        .into_iter()
        .take(max)
        .collect::<Vec<_>>();
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
            "truncated": false,
            "found": 0,
            "shown": 0,
            "reasons": Value::Array(Vec::new()),
            "revision": state.snapshot().revision,
            "runtime": false
        }
    }))
}

fn take_nodes(state: &RepositoryState, kind: &str, path: Option<&str>, max: usize) -> Vec<Value> {
    state
        .graph()
        .nodes()
        .iter()
        .filter(|node| node.kind.as_str() == kind && node.id.as_str().starts_with("symbol:"))
        .filter(|node| path.is_none_or(|filter| view::in_path(node, filter)))
        .take(max)
        .map(|node| {
            json!({
                "id": node.id,
                "label": node.label,
                "kind": node.kind,
                "file": view::file_of(node),
                "span": node.span
            })
        })
        .collect()
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
