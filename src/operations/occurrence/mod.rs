//! Precise occurrence navigation: `(path, line, column)` → symbol → definition.
//!
//! This is not a second reference engine. Resolution reuses the graph
//! occurrence already recorded on reference edges, then an on-disk SCIP
//! index when one is already present. Unresolved stays unresolved: a unique
//! repository name is not a definition.

mod path;
mod position;
mod report;
mod scip;

use crate::engine::RepositoryState;
use crate::operations::optional_str;
use blazingly_json::{Value, json};
use position::Query;
use report::{
    graph_definition, node_json, occurrence_json, optional_query, page_json, page_limit,
    query_json, required_query, scip_definition, sources, span_key,
};
use weavatrix_graph::NodeIndex;

pub(crate) use path::repo_relative_file;

pub(crate) enum InspectSubject {
    Node(NodeIndex),
    Unresolved(Value),
}

pub fn go_to_definition(state: &RepositoryState, args: &Value) -> Result<Value, String> {
    let query = required_query(args)?;
    let loaded = scip::load(state, optional_str(args, "scip_path")?)?;
    if let Some(hit) = position::resolve(state, &query) {
        return Ok(json!({
            "state": "RESOLVED",
            "query": query_json(&query),
            "occurrence": occurrence_json(&hit),
            "definition": graph_definition(state, &hit)?,
            "precision": "parser_plus_graph",
            "semantic_precision": "BOUNDED_STATIC",
            "sources": sources(loaded.as_ref())
        }));
    }
    if let Some(hit) = loaded.as_ref().and_then(|index| index.at_position(&query))
        && hit.definition.is_some()
    {
        return Ok(json!({
            "state": "RESOLVED",
            "query": query_json(&query),
            "occurrence": {
                "role": "usage",
                "span": hit.usage,
                "extractor": "scip",
                "relation": Value::Null
            },
            "definition": scip_definition(state, &hit),
            "precision": "scip",
            "semantic_precision": "BOUNDED_STATIC",
            "sources": sources(loaded.as_ref())
        }));
    }
    Ok(unresolved(
        &query,
        loaded.as_ref(),
        "no occurrence at this position",
    ))
}

pub fn find_references(state: &RepositoryState, args: &Value) -> Result<Value, String> {
    let loaded = scip::load(state, optional_str(args, "scip_path")?)?;
    let (offset, max) = page_limit(args)?;
    let Some(subject) = reference_subject(state, args, loaded.as_ref())? else {
        return scip_only_or_unresolved(state, args, loaded.as_ref());
    };
    let mut rows = graph_references(state, subject.index)?;
    if let Some(symbol) = subject.scip_symbol.as_deref()
        && let Some(index) = loaded.as_ref()
    {
        merge_scip_references(&mut rows, index, symbol);
    } else if let (Some(index), Some(query)) = (loaded.as_ref(), subject.query.as_ref())
        && let Some(symbol) = index.symbol_at(query)
    {
        merge_scip_references(&mut rows, index, symbol);
    }
    rows.sort_by(|left, right| value_span_key(left).cmp(&value_span_key(right)));
    rows.dedup_by(|left, right| value_span_key(left) == value_span_key(right));
    let total = rows.len();
    let page = rows.into_iter().skip(offset).take(max).collect::<Vec<_>>();
    let returned = page.len();
    Ok(json!({
        "state": "RESOLVED",
        "query": subject.query.as_ref().map(query_json),
        "definition": node_json(state.node(subject.index)?),
        "references": page,
        "page": page_json(offset, returned, total),
        "precision": if loaded.is_some() { "parser_plus_graph_plus_scip" } else { "parser_plus_graph" },
        "semantic_precision": "BOUNDED_STATIC",
        "sources": sources(loaded.as_ref())
    }))
}

pub(crate) fn inspect_subject(
    state: &RepositoryState,
    args: &Value,
) -> Result<InspectSubject, String> {
    if let Some(query) = optional_query(args)? {
        let loaded = scip::load(state, optional_str(args, "scip_path")?)?;
        if let Some(hit) = position::resolve(state, &query) {
            return Ok(InspectSubject::Node(hit.index));
        }
        if let Some(hit) = loaded.as_ref().and_then(|index| index.at_position(&query))
            && let Some(span) = hit.definition.as_ref()
        {
            let at = Query {
                path: path::normalize(span.file.as_str()),
                line: span.start.line,
                column: span.start.column,
            };
            if let Some(mapped) = position::resolve(state, &at) {
                return Ok(InspectSubject::Node(mapped.index));
            }
        }
        return Ok(InspectSubject::Unresolved(unresolved(
            &query,
            loaded.as_ref(),
            "no occurrence at this position",
        )));
    }
    let label = optional_str(args, "label")?
        .ok_or_else(|| "inspect_symbol requires label or path, line, and column".to_owned())?;
    Ok(InspectSubject::Node(state.resolve_node(label)?))
}

struct ReferenceSubject {
    index: NodeIndex,
    query: Option<Query>,
    scip_symbol: Option<String>,
}

fn reference_subject(
    state: &RepositoryState,
    args: &Value,
    loaded: Option<&scip::Loaded>,
) -> Result<Option<ReferenceSubject>, String> {
    if let Some(query) = optional_query(args)? {
        if let Some(hit) = position::resolve(state, &query) {
            return Ok(Some(ReferenceSubject {
                index: hit.index,
                scip_symbol: loaded.and_then(|index| index.symbol_at(&query).map(str::to_owned)),
                query: Some(query),
            }));
        }
        if let Some(hit) = loaded.and_then(|index| index.at_position(&query))
            && let Some(span) = hit.definition.as_ref()
        {
            let at = Query {
                path: path::normalize(span.file.as_str()),
                line: span.start.line,
                column: span.start.column,
            };
            if let Some(mapped) = position::resolve(state, &at) {
                return Ok(Some(ReferenceSubject {
                    index: mapped.index,
                    query: Some(query),
                    scip_symbol: Some(hit.symbol),
                }));
            }
        }
        return Ok(None);
    }
    let label = optional_str(args, "label")?
        .ok_or_else(|| "find_references requires label or path, line, and column".to_owned())?;
    Ok(Some(ReferenceSubject {
        index: state.resolve_node(label)?,
        query: None,
        scip_symbol: None,
    }))
}

fn graph_references(state: &RepositoryState, index: NodeIndex) -> Result<Vec<Value>, String> {
    let node = state.node(index)?;
    let mut rows = Vec::new();
    if let Some(span) = node.span.as_ref() {
        rows.push(json!({
            "role": "definition",
            "span": span,
            "extractor": "weavatrix-graph",
            "relation": Value::Null,
            "node": node_json(node)
        }));
    }
    for edge in position::incoming_references(state, index) {
        if let Some(span) = edge.provenance.span.as_ref() {
            rows.push(json!({
                "role": "reference",
                "span": span,
                "extractor": edge.provenance.extractor,
                "relation": edge.kind,
                "node": state.graph().node(edge.source.as_str()).map(node_json)
            }));
        }
    }
    Ok(rows)
}

fn merge_scip_references(rows: &mut Vec<Value>, index: &scip::Loaded, symbol: &str) {
    let known = rows
        .iter()
        .filter_map(|row| row.get("span").and_then(|span| span_from_value(span)))
        .collect::<Vec<_>>();
    for span in index.references(symbol) {
        if known
            .iter()
            .any(|existing| span_key(existing) == span_key(&span))
        {
            continue;
        }
        rows.push(json!({
            "role": "reference",
            "span": span,
            "extractor": "scip",
            "relation": Value::Null,
            "node": Value::Null
        }));
    }
}

fn value_span_key(row: &Value) -> (String, u32, u32) {
    row.get("span")
        .and_then(span_from_value)
        .map(|span| span_key(&span))
        .unwrap_or_default()
}

fn span_from_value(value: &Value) -> Option<weavatrix_graph::SourceSpan> {
    let file = value.get("file")?.as_str()?;
    let start = value.get("start")?;
    let end = value.get("end")?;
    Some(weavatrix_graph::SourceSpan::new(
        file,
        weavatrix_graph::SourcePosition::new(
            u32::try_from(start.get("line")?.as_u64()?).ok()?,
            u32::try_from(start.get("column")?.as_u64()?).ok()?,
        ),
        weavatrix_graph::SourcePosition::new(
            u32::try_from(end.get("line")?.as_u64()?).ok()?,
            u32::try_from(end.get("column")?.as_u64()?).ok()?,
        ),
    ))
}

fn scip_only_or_unresolved(
    state: &RepositoryState,
    args: &Value,
    loaded: Option<&scip::Loaded>,
) -> Result<Value, String> {
    let query = optional_query(args)?;
    if let (Some(query), Some(index)) = (query.as_ref(), loaded)
        && let Some(hit) = index.at_position(query)
    {
        let (offset, max) = page_limit(args)?;
        let mut rows = Vec::new();
        merge_scip_references(&mut rows, index, &hit.symbol);
        rows.sort_by(|left, right| value_span_key(left).cmp(&value_span_key(right)));
        let total = rows.len();
        let page = rows.into_iter().skip(offset).take(max).collect::<Vec<_>>();
        let returned = page.len();
        return Ok(json!({
            "state": "RESOLVED",
            "query": query_json(query),
            "definition": scip_definition(state, &hit),
            "references": page,
            "page": page_json(offset, returned, total),
            "precision": "scip",
            "semantic_precision": "BOUNDED_STATIC",
            "sources": sources(loaded)
        }));
    }
    Ok(json!({
        "state": "UNRESOLVED",
        "query": query.as_ref().map(query_json),
        "definition": Value::Null,
        "references": [],
        "page": page_json(0, 0, 0),
        "precision": "parser_plus_graph",
        "semantic_precision": "BOUNDED_STATIC",
        "sources": sources(loaded),
        "reason": "no occurrence at this position"
    }))
}

fn unresolved(query: &Query, loaded: Option<&scip::Loaded>, reason: &str) -> Value {
    json!({
        "state": "UNRESOLVED",
        "query": query_json(query),
        "occurrence": Value::Null,
        "definition": Value::Null,
        "precision": "parser_plus_graph",
        "semantic_precision": "BOUNDED_STATIC",
        "sources": sources(loaded),
        "reason": reason
    })
}
