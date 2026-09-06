//! Precise occurrence navigation: `(path, line, column)` → symbol → definition.
//!
//! This is not a second reference engine. Resolution reuses the graph
//! occurrence already recorded on reference edges, then an on-disk SCIP
//! index when one is already present. Unresolved stays unresolved: a unique
//! repository name is not a definition.

mod path;
mod position;
mod references;
mod report;
mod scip;

use crate::engine::RepositoryState;
use crate::operations::optional_str;
use blazingly_json::{Value, json};
use position::Query;
use references::unresolved;
use report::{
    graph_definition, occurrence_json, optional_query, query_json, required_query, scip_definition,
    sources,
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
    references::find_references(state, args)
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
