mod view;
mod walk;

use crate::engine::RepositoryState;
use crate::operations::{arg_str, optional_str, optional_u64, reject_unknown_arguments};
use blazingly_json::{Value, json};
use weavatrix_graph::Node;

pub(super) fn inventory(state: &RepositoryState, args: &Value) -> Result<Value, String> {
    reject_unknown_arguments("dify_inventory", args, &["path", "max_results"])?;
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
        .filter(|node| node.kind.as_str() == "dify.app")
        .filter(|node| path.is_none_or(|filter| view::in_path(node, filter)))
        .collect::<Vec<_>>();
    let found = all.len();
    let apps = all
        .into_iter()
        .take(max)
        .map(|node| view::app_row(state, node))
        .collect::<Vec<_>>();
    let decode_cut = state
        .snapshot()
        .diagnostics
        .iter()
        .any(|item| item.code == "dify.truncated" || item.code == "dify.limit");
    let page_cut = found > apps.len();
    let mut reasons = Vec::new();
    if page_cut {
        reasons.push("max_results");
    }
    if decode_cut {
        reasons.push("decode");
    }
    Ok(json!({
        "apps": apps,
        "coverage": view::coverage_from(state, path),
        "bounds": {
            "truncated": page_cut || decode_cut,
            "found": found,
            "shown": apps.len(),
            "reasons": reasons,
            "revision": state.snapshot().revision,
            "runtime": false
        },
        "diagnostics": state
            .snapshot()
            .diagnostics
            .iter()
            .filter(|item| item.code.starts_with("dify.") || item.code.starts_with("yaml."))
            .collect::<Vec<_>>()
    }))
}

pub(super) fn trace(state: &RepositoryState, args: &Value) -> Result<Value, String> {
    walk::trace(state, args)
}

pub(super) fn context(state: &RepositoryState, args: &Value) -> Result<Value, String> {
    reject_unknown_arguments("dify_context", args, &["label", "task", "max_related"])?;
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
                Some(
                    "depends_on_output"
                        | "reads_variable"
                        | "writes_variable"
                        | "binds_input"
                        | "uses_model"
                        | "invokes_tool"
                        | "queries_knowledge"
                )
            )
        }),
        max_related,
    );
    let (variables, var_cut) = take_related(
        domains.iter().filter(|item| {
            item["name"].as_str().is_some_and(|name| {
                name.starts_with("selector:")
                    || name.starts_with("marker:")
                    || name.starts_with("conversation:")
                    || name.starts_with("template:")
                    || name.starts_with("binding:")
                    || name.starts_with("sys:")
            })
        }),
        max_related,
    );
    let mut gaps = vec!["runtime response shape is not provided".to_owned()];
    for item in &domains {
        if item["name"].as_str().is_some_and(|name| {
            name.starts_with("unresolved:")
                || name == "semantics:unsupported"
                || name == "semantics:structure_only"
        }) {
            gaps.push(item["name"].as_str().unwrap_or("limited").to_owned());
        }
    }
    gaps.sort();
    gaps.dedup();
    let consumers = if impact_task(task) {
        consumers(state, node, max_related)
    } else {
        Vec::new()
    };
    let fragments = selected_fragments(state, node);
    let truncated = dep_cut || var_cut;
    Ok(json!({
        "selected": {
            "id": node.id,
            "label": node.label,
            "kind": node.kind,
            "span": node.span
        },
        "task": task,
        "dependencies": dependencies,
        "variables": variables,
        "consumers": consumers,
        "fragments": fragments,
        "gaps": gaps,
        "coverage": view::coverage_from(state, node.span.as_ref().map(|span| span.file.as_str())),
        "bounds": {
            "truncated": truncated,
            "found": dependencies.len() + variables.len() + consumers.len(),
            "shown": dependencies.len() + variables.len() + consumers.len(),
            "reasons": if truncated { vec!["max_related"] } else { Vec::<&str>::new() },
            "revision": state.snapshot().revision,
            "complete": !truncated && gaps.len() == 1,
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
                    "flows_to"
                        | "depends_on_output"
                        | "reads_variable"
                        | "writes_variable"
                        | "binds_input"
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
    crate::language::dify_secret_label(text)
}
