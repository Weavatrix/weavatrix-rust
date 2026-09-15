mod view;
mod walk;

use crate::engine::RepositoryState;
use crate::operations::{arg_str, optional_str, optional_u64, reject_unknown_arguments};
use blazingly_json::{Value, json};

pub(super) fn inventory(state: &RepositoryState, args: &Value) -> Result<Value, String> {
    reject_unknown_arguments("n8n_inventory", args, &["path", "max_results"])?;
    let path = optional_str(args, "path")?;
    let max = usize::try_from(optional_u64(args, "max_results")?.unwrap_or(200))
        .map_err(|_| "max_results is too large")?;
    if max == 0 || max > 500 {
        return Err("max_results must be between 1 and 500".to_owned());
    }
    let workflows = state
        .graph()
        .nodes()
        .iter()
        .filter(|node| node.kind.as_str() == "n8n.workflow")
        .filter(|node| path.is_none_or(|filter| view::in_path(node, filter)))
        .take(max)
        .map(|node| view::workflow_row(state, node))
        .collect::<Vec<_>>();
    let truncated = state
        .snapshot()
        .diagnostics
        .iter()
        .any(|item| item.code == "n8n.truncated" || item.code == "n8n.limit");
    Ok(json!({
        "workflows": workflows,
        "coverage": view::coverage_from(state, path),
        "bounds": {"truncated": truncated, "runtime": false},
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
    let index = state.resolve_node(label)?;
    let node = state.node(index)?;
    let domains = view::owned_domains(state, node.id.as_str())
        .into_iter()
        .filter(|item| !view::secret_domain(item))
        .collect::<Vec<_>>();
    let dependencies = domains
        .iter()
        .filter(|item| {
            matches!(
                item["relation"].as_str(),
                Some("depends_on_output" | "reads_field" | "calls_workflow" | "configured_with")
            )
        })
        .take(max_related)
        .cloned()
        .collect::<Vec<_>>();
    let expressions = domains
        .iter()
        .filter(|item| {
            item["relation"].as_str() == Some("reads_field")
                || item["name"]
                    .as_str()
                    .is_some_and(|name| name.contains('.') || name.starts_with('$'))
        })
        .take(max_related)
        .cloned()
        .collect::<Vec<_>>();
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
        "gaps": unknown,
        "coverage": view::coverage_from(state, node.span.as_ref().map(|span| span.file.as_str())),
        "bounds": {"truncated": false, "runtime": false}
    }))
}
