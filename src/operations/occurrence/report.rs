use super::position::{GraphHit, Query, Role};
use super::scip::{Loaded, ScipHit};
use crate::engine::RepositoryState;
use crate::operations::{arg_u64, optional_str, optional_u64};
use blazingly_json::{Value, json};
use weavatrix_graph::{Node, SourceSpan};

pub(super) fn required_query(args: &Value) -> Result<Query, String> {
    optional_query(args)?.ok_or_else(|| "path, line, and column are required".to_owned())
}

pub(super) fn optional_query(args: &Value) -> Result<Option<Query>, String> {
    let path = optional_str(args, "path")?;
    let line = optional_u64(args, "line")?;
    let column = optional_u64(args, "column")?;
    match (path, line, column) {
        (None, None, None) => Ok(None),
        (Some(path), Some(line), Some(column)) => Ok(Some(Query::new(path, line, column)?)),
        _ => Err("path, line, and column must be supplied together".to_owned()),
    }
}

pub(super) fn sources(scip: Option<&Loaded>) -> Value {
    json!({
        "graph": true,
        "scip": scip.map_or_else(
            || json!({"present": false, "consulted": false}),
            |loaded| json!({
                "present": true,
                "consulted": true,
                "path": loaded.relative
            })
        ),
        "stack_graphs": {
            "present": false,
            "consulted": false,
            "reason": "github/stack-graphs is archived; Weavatrix does not take it as a live dependency"
        }
    })
}

pub(super) fn query_json(query: &Query) -> Value {
    json!({"path": query.path, "line": query.line, "column": query.column})
}

pub(super) fn node_json(node: &Node) -> Value {
    json!({
        "id": node.id,
        "label": node.label,
        "kind": node.kind,
        "span": node.span
    })
}

pub(super) fn graph_definition(state: &RepositoryState, hit: &GraphHit) -> Result<Value, String> {
    Ok(node_json(state.node(hit.index)?))
}

pub(super) fn scip_definition(state: &RepositoryState, hit: &ScipHit) -> Value {
    let mapped = hit.definition.as_ref().and_then(|span| {
        let query = Query {
            path: super::path::normalize(span.file.as_str()),
            line: span.start.line,
            column: span.start.column,
        };
        super::position::resolve(state, &query)
            .and_then(|graph| state.node(graph.index).ok())
            .map(node_json)
    });
    json!({
        "id": mapped.as_ref().and_then(|node| node.get("id").cloned()),
        "label": mapped.as_ref().and_then(|node| node.get("label").cloned()),
        "kind": mapped.as_ref().and_then(|node| node.get("kind").cloned()),
        "span": hit.definition,
        "scip_symbol": hit.symbol
    })
}

pub(super) fn occurrence_json(hit: &GraphHit) -> Value {
    json!({
        "role": match hit.role {
            Role::Definition => "definition",
            Role::Usage => "usage",
        },
        "span": hit.span,
        "extractor": hit.extractor,
        "relation": hit.relation
    })
}

pub(super) fn page_limit(args: &Value) -> Result<(usize, usize), String> {
    let max = usize::try_from(arg_u64(args, "max_results").unwrap_or(50)).unwrap_or(50);
    if max == 0 || max > 500 {
        return Err("max_results must be between 1 and 500".to_owned());
    }
    Ok((super::super::graph::page_offset(args)?, max))
}

pub(super) fn page_json(offset: usize, returned: usize, total: usize) -> Value {
    let end = offset.saturating_add(returned);
    json!({
        "offset": offset,
        "returned": returned,
        "total": total,
        "has_more": end < total,
        "next_cursor": (end < total).then(|| format!("v1:{end}"))
    })
}

pub(super) fn span_key(span: &SourceSpan) -> (String, u32, u32) {
    (
        super::path::normalize(span.file.as_str()),
        span.start.line,
        span.start.column,
    )
}
