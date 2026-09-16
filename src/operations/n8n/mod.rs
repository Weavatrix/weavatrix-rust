mod view;
mod walk;

use crate::engine::RepositoryState;
use crate::operations::{arg_str, optional_str, optional_u64, reject_unknown_arguments};
use blazingly_json::{Value, json};
use weavatrix_graph::Node;

pub(super) fn inventory(state: &RepositoryState, args: &Value) -> Result<Value, String> {
    reject_unknown_arguments("n8n_inventory", args, &["path", "max_results"])?;
    let path = optional_str(args, "path")?;
    let max = usize::try_from(optional_u64(args, "max_results")?.unwrap_or(200))
        .map_err(|_| "max_results is too large")?;
    if max == 0 || max > 500 {
        return Err("max_results must be between 1 and 500".to_owned());
    }
    let all = state
        .graph()
        .nodes()
        .iter()
        .filter(|node| node.kind.as_str() == "n8n.workflow")
        .filter(|node| path.is_none_or(|filter| view::in_path(node, filter)))
        .collect::<Vec<_>>();
    let found = all.len();
    let workflows = all
        .into_iter()
        .take(max)
        .map(|node| view::workflow_row(state, node))
        .collect::<Vec<_>>();
    let decode_cut = state
        .snapshot()
        .diagnostics
        .iter()
        .any(|item| item.code == "n8n.truncated" || item.code == "n8n.limit");
    let page_cut = found > workflows.len();
    let mut reasons = Vec::new();
    if page_cut {
        reasons.push("max_results");
    }
    if decode_cut {
        reasons.push("decode");
    }
    Ok(json!({
        "workflows": workflows,
        "coverage": view::coverage_from(state, path),
        "bounds": {
            "truncated": page_cut || decode_cut,
            "found": found,
            "shown": workflows.len(),
            "reasons": reasons,
            "revision": state.snapshot().revision,
            "runtime": false
        },
        "diagnostics": state
            .snapshot()
            .diagnostics
            .iter()
            .filter(|item| item.code.starts_with("n8n."))
            .collect::<Vec<_>>()
    }))
}

pub(super) fn trace(state: &RepositoryState, args: &Value) -> Result<Value, String> {
    walk::trace(state, args)
}

pub(super) fn context(state: &RepositoryState, args: &Value) -> Result<Value, String> {
    reject_unknown_arguments("n8n_context", args, &["label", "task", "max_related"])?;
    let label = arg_str(args, "label")?;
    let task = optional_str(args, "task")?.unwrap_or("inspect");
    let max_related = usize::try_from(optional_u64(args, "max_related")?.unwrap_or(24))
        .map_err(|_| "max_related is too large")?;
    if max_related == 0 || max_related > 200 {
        return Err("max_related must be between 1 and 200".to_owned());
    }
    let index = state.resolve_node(label)?;
    let node = state.node(index)?;
    let domains = view::owned_domains(state, node.id.as_str())
        .into_iter()
        .filter(|item| !view::secret_domain(item))
        .collect::<Vec<_>>();
    let (dependencies, dep_cut) = take_related(
        domains.iter().filter(|item| {
            matches!(
                item["relation"].as_str(),
                Some("depends_on_output" | "reads_field" | "calls_workflow" | "configured_with")
            )
        }),
        max_related,
    );
    let (expressions, expr_cut) = take_related(
        domains.iter().filter(|item| {
            item["relation"].as_str() == Some("reads_field")
                || item["name"]
                    .as_str()
                    .is_some_and(|name| name.contains('.') || name.starts_with('$'))
        }),
        max_related,
    );
    let mut unknown = vec!["runtime response shape is not provided".to_owned()];
    for item in &domains {
        if item["name"]
            .as_str()
            .is_some_and(|name| name.starts_with("unresolved:") || name.contains(":dynamic"))
        {
            unknown.push(item["name"].as_str().unwrap_or("unresolved").to_owned());
        }
        if item["name"].as_str() == Some("semantics:unsupported")
            || item["name"].as_str() == Some("semantics:structure_only")
        {
            unknown.push(item["name"].as_str().unwrap_or("limited").to_owned());
        }
    }
    unknown.sort();
    unknown.dedup();
    let consumers = if impact_task(task) {
        consumers(state, node, max_related)
    } else {
        Vec::new()
    };
    let fragments = selected_fragments(state, node);
    let truncated = dep_cut || expr_cut || consumers.len() == max_related && impact_task(task);
    Ok(json!({
        "selected": {
            "id": node.id,
            "label": node.label,
            "kind": node.kind,
            "span": node.span
        },
        "task": task,
        "dependencies": dependencies,
        "expressions": expressions,
        "consumers": consumers,
        "fragments": fragments,
        "gaps": unknown,
        "coverage": view::coverage_from(state, node.span.as_ref().map(|span| span.file.as_str())),
        "bounds": {
            "truncated": truncated,
            "found": dependencies.len() + expressions.len() + consumers.len(),
            "shown": dependencies.len() + expressions.len() + consumers.len(),
            "reasons": if truncated { vec!["max_related"] } else { Vec::<&str>::new() },
            "revision": state.snapshot().revision,
            "complete": !truncated && unknown.len() == 1,
            "runtime": false
        }
    }))
}

fn take_related<'a>(items: impl Iterator<Item = &'a Value>, max: usize) -> (Vec<Value>, bool) {
    let all = items.cloned().collect::<Vec<_>>();
    let cut = all.len() > max;
    (all.into_iter().take(max).collect(), cut)
}

fn impact_task(task: &str) -> bool {
    let lower = task.to_ascii_lowercase();
    ["impact", "change", "delete", "consum", "rename", "remove"]
        .iter()
        .any(|marker| lower.contains(marker))
}

fn consumers(state: &RepositoryState, node: &Node, max: usize) -> Vec<Value> {
    state
        .graph()
        .edges()
        .iter()
        .filter(|edge| {
            edge.target.as_str() == node.id.as_str()
                && matches!(
                    edge.kind.as_str(),
                    "flows_to" | "depends_on_output" | "reads_field" | "calls_workflow"
                )
        })
        .filter_map(|edge| {
            let source = state.graph().node(edge.source.as_str())?;
            Some(json!({
                "id": source.id,
                "label": source.label,
                "kind": source.kind,
                "relation": edge.kind,
                "span": source.span
            }))
        })
        .take(max)
        .collect()
}

fn selected_fragments(state: &RepositoryState, node: &Node) -> Vec<Value> {
    if node.span.is_none() {
        return Vec::new();
    }
    let Ok(source) = crate::operations::source::read_source(
        state,
        &json!({
            "label": node.id.as_str(),
            "before": 1,
            "after": 12
        }),
    ) else {
        return Vec::new();
    };
    let lines = source["lines"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|line| {
            let text = line["text"].as_str()?;
            if looks_secret_line(text) {
                return Some(json!({
                    "line": line["line"],
                    "text": "[redacted]"
                }));
            }
            Some(line.clone())
        })
        .take(16)
        .collect::<Vec<_>>();
    if lines.is_empty() {
        Vec::new()
    } else {
        vec![json!({
            "path": source["path"],
            "start_line": source["start_line"],
            "lines": lines
        })]
    }
}

fn looks_secret_line(text: &str) -> bool {
    let lower = text.to_ascii_lowercase();
    lower.contains("password")
        || lower.contains("token")
        || lower.contains("secret")
        || lower.contains("api_key")
        || lower.contains("apikey")
        || lower.contains("authorization")
        || (lower.contains("://") && lower.contains('@'))
}
