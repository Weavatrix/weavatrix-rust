mod view;

use crate::engine::RepositoryState;
use crate::language::web3::silent_misdecode_example;
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
    let max = bounded(optional_u64(args, "max_nodes")?.unwrap_or(64), 200)?;
    let index = state.resolve_node(label)?;
    let node = state.node(index)?;
    let mut steps = Vec::new();
    steps.push(json!({
        "id": node.id,
        "label": node.label,
        "kind": node.kind,
        "span": node.span
    }));
    for edge in state.graph().edges() {
        if steps.len() >= max {
            break;
        }
        if edge.source.as_str() == node.id.as_str() || edge.target.as_str() == node.id.as_str() {
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
                    "span": next.span
                }));
            }
        }
    }
    Ok(json!({
        "selected": { "id": node.id, "label": node.label, "kind": node.kind, "span": node.span },
        "steps": steps,
        "coverage": view::coverage(state),
        "bounds": {
            "truncated": steps.len() >= max,
            "found": steps.len(),
            "shown": steps.len(),
            "reasons": if steps.len() >= max { vec!["max_nodes"] } else { Vec::<&str>::new() },
            "revision": state.snapshot().revision,
            "runtime": false
        }
    }))
}

pub(super) fn impact(state: &RepositoryState, args: &Value) -> Result<Value, String> {
    reject_unknown_arguments(
        "web3_impact",
        args,
        &[
            "path",
            "baseline",
            "candidate",
            "provider",
            "max_results",
            "task",
        ],
    )?;
    let task = optional_str(args, "task")?.unwrap_or("change-event");
    let max = bounded(optional_u64(args, "max_results")?.unwrap_or(50), 200)?;
    let baseline =
        optional_str(args, "baseline")?.or_else(|| optional_str(args, "path").ok().flatten());
    let candidate =
        optional_str(args, "candidate")?.or_else(|| optional_str(args, "provider").ok().flatten());
    let changes = event_changes(state, baseline, candidate)
        .into_iter()
        .take(max)
        .collect::<Vec<_>>();
    Ok(json!({
        "task": task,
        "changes": changes,
        "coverage": view::coverage(state),
        "bounds": {
            "truncated": false,
            "found": changes.len(),
            "shown": changes.len(),
            "reasons": Value::Array(Vec::new()),
            "revision": state.snapshot().revision,
            "runtime": false,
            "deployment": "not_provided"
        }
    }))
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

fn event_changes(
    state: &RepositoryState,
    baseline: Option<&str>,
    candidate: Option<&str>,
) -> Vec<Value> {
    let events = state
        .graph()
        .nodes()
        .iter()
        .filter(|node| {
            node.kind.as_str() == "web3.abi.event" && node.id.as_str().starts_with("symbol:")
        })
        .filter(|node| {
            baseline.is_none_or(|filter| view::in_path(node, filter))
                || candidate.is_none_or(|filter| view::in_path(node, filter))
        })
        .collect::<Vec<_>>();
    let mut changes = Vec::new();
    for left in &events {
        for right in &events {
            if left.id.as_str() >= right.id.as_str() {
                continue;
            }
            if signature(left) != signature(right) {
                continue;
            }
            let left_layout = layout(state, left);
            let right_layout = layout(state, right);
            if left_layout.is_empty() || left_layout == right_layout {
                continue;
            }
            let mut consumers = view::consumers_of(state, left.id.as_str());
            consumers.extend(view::consumers_of(state, right.id.as_str()));
            changes.push(json!({
                "kind": "EVENT_LAYOUT_CHANGED",
                "member": signature(left),
                "topicSignatureUnchanged": true,
                "baseline": { "id": left.id, "file": view::file_of(left), "layout": left_layout },
                "candidate": { "id": right.id, "file": view::file_of(right), "layout": right_layout },
                "consumers": consumers,
                "effect": "requires_decoder_review",
                "possibleConsequence": "silent_misdecode",
                "counterexample": silent_misdecode_example(),
                "deployment": "not_provided"
            }));
        }
    }
    let _ = (baseline, candidate);
    changes
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

fn signature(node: &Node) -> String {
    node.label.clone()
}

fn layout(state: &RepositoryState, node: &Node) -> String {
    view::domain_names(state, node.id.as_str())
        .into_iter()
        .find_map(|name| name.strip_prefix("web3.event_layout:").map(str::to_owned))
        .unwrap_or_default()
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
