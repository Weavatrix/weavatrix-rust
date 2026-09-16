mod docs;
mod resolve;
mod view;
mod walk;

use crate::engine::RepositoryState;
use crate::operations::{arg_str, optional_str, optional_u64, reject_unknown_arguments};
use blazingly_json::{Value, json};
use weavatrix_graph::Node;

pub(in crate::operations) use docs::affected;

pub(super) fn inventory(state: &RepositoryState, args: &Value) -> Result<Value, String> {
    reject_unknown_arguments("diagram_inventory", args, &["path", "max_results"])?;
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
        .filter(|node| node.kind.as_str() == "mermaid.diagram")
        .filter(|node| path.is_none_or(|filter| view::in_path(node, filter)))
        .collect::<Vec<_>>();
    let found = all.len();
    let diagrams = all
        .into_iter()
        .take(max)
        .map(|node| view::diagram_row(state, node))
        .collect::<Vec<_>>();
    let decode_cut = state
        .snapshot()
        .diagnostics
        .iter()
        .any(|item| item.code == "mermaid.limit" || item.code == "mermaid.unsupported");
    let page_cut = found > diagrams.len();
    let mut reasons = Vec::new();
    if page_cut {
        reasons.push("max_results");
    }
    if decode_cut {
        reasons.push("decode");
    }
    Ok(json!({
        "diagrams": diagrams,
        "bindings": bindings(state, path, max),
        "bounds": {
            "truncated": page_cut || decode_cut,
            "found": found,
            "shown": diagrams.len(),
            "reasons": reasons,
            "revision": state.snapshot().revision,
            "runtime": false
        }
    }))
}

pub(super) fn trace(state: &RepositoryState, args: &Value) -> Result<Value, String> {
    walk::trace(state, args)
}

pub(super) fn context(state: &RepositoryState, args: &Value) -> Result<Value, String> {
    reject_unknown_arguments("diagram_context", args, &["label", "task", "max_related"])?;
    let label = arg_str(args, "label")?;
    let task = optional_str(args, "task")?.unwrap_or("inspect");
    let max_related = usize::try_from(optional_u64(args, "max_related")?.unwrap_or(24))
        .map_err(|_| "max_related is too large")?;
    if max_related == 0 || max_related > 200 {
        return Err("max_related must be between 1 and 200".to_owned());
    }
    let index = state.resolve_node(label)?;
    let node = state.node(index)?;
    let domains = view::owned_domains(state, node.id.as_str());
    let relations = declared_neighbors(state, node, max_related);
    let selected_bindings = bindings_for(state, node);
    let fragments = fragments(state, node);
    let gaps = gaps(task, &selected_bindings);
    Ok(json!({
        "selected": {
            "id": node.id,
            "label": node.label,
            "kind": node.kind,
            "span": node.span,
            "display": view::domain_name(&domains, "label:")
        },
        "task": task,
        "relations": relations,
        "bindings": selected_bindings,
        "fragments": fragments,
        "gaps": gaps,
        "bounds": {
            "truncated": false,
            "revision": state.snapshot().revision,
            "complete": gaps.is_empty(),
            "runtime": false,
            "plane": "declared_architecture"
        }
    }))
}

fn bindings(state: &RepositoryState, path: Option<&str>, max: usize) -> Vec<Value> {
    state
        .graph()
        .nodes()
        .iter()
        .filter(|node| node.kind.as_str() == "mermaid.binding")
        .filter(|node| path.is_none_or(|filter| view::in_path(node, filter)))
        .take(max)
        .map(|node| resolve::binding_row(state, node))
        .collect()
}

fn bindings_for(state: &RepositoryState, node: &Node) -> Vec<Value> {
    bindings(state, None, 24)
        .into_iter()
        .filter(|item| {
            item["element"].as_str() == Some(node.label.as_str())
                || item["diagram"].as_str() == Some(node.label.as_str())
        })
        .collect()
}

fn declared_neighbors(state: &RepositoryState, node: &Node, max: usize) -> Vec<Value> {
    state
        .graph()
        .edges()
        .iter()
        .filter(|edge| {
            edge.kind.as_str() == "declared_architecture"
                && (edge.source.as_str() == node.id.as_str()
                    || edge.target.as_str() == node.id.as_str())
        })
        .filter_map(|edge| {
            let other = if edge.source.as_str() == node.id.as_str() {
                state.graph().node(edge.target.as_str())?
            } else {
                state.graph().node(edge.source.as_str())?
            };
            Some(json!({
                "id": other.id,
                "label": other.label,
                "relation": edge.kind,
                "detail": edge.provenance.detail,
                "span": other.span
            }))
        })
        .take(max)
        .collect()
}

fn fragments(state: &RepositoryState, node: &Node) -> Vec<Value> {
    if node.span.is_none() {
        return Vec::new();
    }
    let Ok(source) = crate::operations::source::read_source(
        state,
        &json!({"label": node.id.as_str(), "before": 1, "after": 12}),
    ) else {
        return Vec::new();
    };
    vec![json!({
        "path": source["path"],
        "start_line": source["start_line"],
        "lines": source["lines"]
    })]
}

fn gaps(task: &str, bindings: &[Value]) -> Vec<String> {
    let mut gaps = vec!["drawn arrows are not production Calls".to_owned()];
    if !task.is_empty() && bindings.iter().all(|item| item["status"] != "exact") {
        gaps.push("no exact implementation binding for this element".to_owned());
    }
    if bindings.iter().any(|item| item["status"] == "ambiguous") {
        gaps.push("binding is ambiguous".to_owned());
    }
    gaps
}
