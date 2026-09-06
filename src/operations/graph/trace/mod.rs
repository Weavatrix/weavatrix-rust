//! Exact endpoint lookup and bounded execution-path tracing.

mod execution;
mod lookup;

use crate::engine::RepositoryState;
use crate::operations::{arg_u64, optional_str};
use blazingly_json::{Value, json};
use execution::{execution_path, source_excerpts};
use lookup::{
    ambiguous, ambiguous_handlers, distinct_handler_files, endpoint_matches_handler,
    exposing_handlers, matching_endpoints, normalize_hint, normalize_method, normalize_path,
    not_found, path_match,
};

pub fn endpoint(state: &RepositoryState, args: &Value) -> Result<Value, String> {
    let path = normalize_path(crate::operations::arg_str(args, "path")?);
    let method = optional_str(args, "method")?.map(normalize_method);
    let match_mode = path_match(args)?;
    let handler_hint = optional_str(args, "handler_file")?.map(normalize_hint);
    let query = json!({
        "path": path,
        "method": method,
        "match": match_mode.as_str(),
        "handler_file": handler_hint
    });

    let mut candidates = matching_endpoints(state, args, &path, method.as_deref(), match_mode);
    if let Some(hint) = handler_hint.as_deref() {
        candidates.retain(|index| endpoint_matches_handler(state, *index, hint));
    }
    if candidates.is_empty() {
        return Ok(not_found(state, query));
    }
    if candidates.len() > 1 {
        return Ok(ambiguous(state, query, &candidates));
    }

    let endpoint_index = candidates[0];
    let handlers = exposing_handlers(state, endpoint_index, handler_hint.as_deref());
    if handlers.is_empty() && handler_hint.is_some() {
        return Ok(not_found(state, query));
    }
    let handler_files = distinct_handler_files(state, &handlers);
    if handler_hint.is_none() && handler_files.len() > 1 {
        return Ok(ambiguous_handlers(query, &handler_files));
    }

    let depth = usize::try_from(arg_u64(args, "max_depth").unwrap_or(4)).unwrap_or(4);
    let max_nodes = usize::try_from(arg_u64(args, "max_nodes").unwrap_or(80)).unwrap_or(80);
    let (ordered, edge_set, truncated, omitted_count) =
        execution_path(state, endpoint_index, &handlers, depth, max_nodes.max(1));

    let endpoint = state
        .graph()
        .node_at(endpoint_index)
        .ok_or_else(|| "endpoint node missing".to_owned())?;
    let nodes = ordered
        .iter()
        .filter_map(|index| state.graph().node_at(*index))
        .collect::<Vec<_>>();
    let edges = edge_set
        .into_iter()
        .filter_map(|index| state.graph().edge_at(index))
        .collect::<Vec<_>>();
    let excerpts = source_excerpts(state, args, &nodes);
    Ok(json!({
        "state": "COMPLETE",
        "query": query,
        "endpoint": endpoint,
        "nodes": nodes,
        "edges": edges,
        "source_excerpts": excerpts,
        "truncated": truncated,
        "omitted_count": omitted_count,
        "has_more": truncated,
        "completeness": {
            "complete": !truncated,
            "reason": if truncated {
                "bounded execution-path cap reached"
            } else {
                "bounded outgoing call graph exhausted"
            }
        },
        "precision": "bounded_static_execution_path",
        "dynamic_dispatch": {
            "evaluated": true,
            "scope": "repository_static_and_declared_runtime_evidence",
            "matches": 0,
            "runtime_evidence_present": false,
            "static_evidence_preserved": true
        }
    }))
}
