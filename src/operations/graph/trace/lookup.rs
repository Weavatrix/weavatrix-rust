use crate::engine::RepositoryState;
use crate::operations::{node_path, optional_str};
use blazingly_json::{Value, json};
use std::collections::BTreeSet;
use weavatrix_graph::{EdgeKind, GraphView, NodeIndex, NodeKind};

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum PathMatch {
    Exact,
    Prefix,
    Suffix,
}

impl PathMatch {
    pub(super) fn as_str(self) -> &'static str {
        match self {
            Self::Exact => "exact",
            Self::Prefix => "prefix",
            Self::Suffix => "suffix",
        }
    }
}

pub(super) fn path_match(args: &Value) -> Result<PathMatch, String> {
    match optional_str(args, "match")?.unwrap_or("exact") {
        "exact" => Ok(PathMatch::Exact),
        "prefix" => Ok(PathMatch::Prefix),
        "suffix" => Ok(PathMatch::Suffix),
        other => Err(format!(
            "match must be one of exact, prefix, or suffix; got {other}"
        )),
    }
}

pub(super) fn matching_endpoints(
    state: &RepositoryState,
    args: &Value,
    path: &str,
    method: Option<&str>,
    mode: PathMatch,
) -> Vec<NodeIndex> {
    let mut matched = state
        .graph()
        .nodes()
        .iter()
        .enumerate()
        .filter(|(slot, node)| {
            node.kind == NodeKind::Endpoint
                && crate::operations::node_is_visible(state, *slot, args)
                && endpoint_label_matches(&node.label, path, method, mode)
        })
        .map(|(slot, _)| NodeIndex::new(u32::try_from(slot).unwrap_or(u32::MAX)))
        .collect::<Vec<_>>();
    matched.sort_by_key(|index| {
        state
            .graph()
            .node_at(*index)
            .map(|node| node.label.as_str().to_owned())
            .unwrap_or_default()
    });
    matched
}

fn endpoint_label_matches(label: &str, path: &str, method: Option<&str>, mode: PathMatch) -> bool {
    let (endpoint_method, endpoint_path) = split_endpoint_label(label);
    if method.is_some_and(|wanted| endpoint_method != wanted) {
        return false;
    }
    let route = normalize_path(endpoint_path);
    match mode {
        PathMatch::Exact => route == path,
        PathMatch::Prefix => route.starts_with(path),
        PathMatch::Suffix => route == path || route.ends_with(path),
    }
}

fn split_endpoint_label(label: &str) -> (&str, &str) {
    label
        .split_once(' ')
        .map_or(("ANY", label), |(method, path)| (method, path))
}

pub(super) fn normalize_path(path: &str) -> String {
    let trimmed = path.trim();
    if trimmed.is_empty() {
        return "/".to_owned();
    }
    if trimmed.starts_with('/') {
        trimmed.to_owned()
    } else {
        format!("/{trimmed}")
    }
}

pub(super) fn normalize_method(method: &str) -> String {
    method.trim().to_ascii_uppercase()
}

pub(super) fn normalize_hint(hint: &str) -> String {
    hint.trim()
        .trim_start_matches("./")
        .replace('\\', "/")
        .to_ascii_lowercase()
}

fn matches_handler_hint(file: &str, hint: &str) -> bool {
    let file = file.replace('\\', "/").to_ascii_lowercase();
    file == hint || file.ends_with(&format!("/{hint}"))
}

pub(super) fn endpoint_matches_handler(
    state: &RepositoryState,
    index: NodeIndex,
    hint: &str,
) -> bool {
    !exposing_handlers(state, index, Some(hint)).is_empty()
}

pub(super) fn exposing_handlers(
    state: &RepositoryState,
    endpoint: NodeIndex,
    hint: Option<&str>,
) -> Vec<NodeIndex> {
    let mut handlers = Vec::new();
    for edge in state.graph().incoming_edges(endpoint) {
        let Some(item) = state.graph().edge_at(edge) else {
            continue;
        };
        if item.kind != EdgeKind::Exposes {
            continue;
        }
        let Some(source) = state.graph().node_index(item.source.as_str()) else {
            continue;
        };
        let Some(node) = state.graph().node_at(source) else {
            continue;
        };
        if is_structural(&node.kind) {
            continue;
        }
        if hint.is_some_and(|hint| {
            node_path(node).is_none_or(|path| !matches_handler_hint(path, hint))
        }) {
            continue;
        }
        handlers.push(source);
    }
    handlers.sort_by_key(|index| {
        let node = state.graph().node_at(*index);
        (
            handler_rank(node.map(|node| &node.kind).unwrap_or(&NodeKind::File)),
            node.and_then(node_path).unwrap_or_default().to_owned(),
            node.map(|node| node.id.as_str().to_owned())
                .unwrap_or_default(),
        )
    });
    handlers.dedup();
    handlers
}

pub(super) fn distinct_handler_files(
    state: &RepositoryState,
    handlers: &[NodeIndex],
) -> BTreeSet<String> {
    handlers
        .iter()
        .filter_map(|index| state.graph().node_at(*index))
        .filter_map(node_path)
        .map(|path| path.replace('\\', "/").to_ascii_lowercase())
        .collect()
}

fn handler_rank(kind: &NodeKind) -> u8 {
    match kind {
        NodeKind::Function | NodeKind::Method => 0,
        NodeKind::Struct => 1,
        NodeKind::File => 2,
        _ => 3,
    }
}

pub(super) fn is_structural(kind: &NodeKind) -> bool {
    matches!(
        kind,
        NodeKind::Repository | NodeKind::Package | NodeKind::Endpoint
    )
}

pub(super) fn not_found(state: &RepositoryState, query: Value) -> Value {
    let available = state
        .graph()
        .nodes()
        .iter()
        .filter(|node| node.kind == NodeKind::Endpoint)
        .take(25)
        .map(|node| node.label.as_str())
        .collect::<Vec<_>>();
    json!({
        "state": "NOT_FOUND",
        "query": query,
        "endpoint": Value::Null,
        "nodes": [],
        "edges": [],
        "source_excerpts": [],
        "available_endpoints": available,
        "truncated": false,
        "omitted_count": 0,
        "has_more": false,
        "completeness": {
            "complete": true,
            "reason": "no endpoint matched the normalized query"
        },
        "precision": "exact_static_endpoint_lookup",
        "dynamic_dispatch": {
            "evaluated": true,
            "scope": "repository_static_and_declared_runtime_evidence",
            "matches": 0,
            "runtime_evidence_present": false
        },
        "source_mutation": "NONE"
    })
}

pub(super) fn ambiguous(state: &RepositoryState, query: Value, candidates: &[NodeIndex]) -> Value {
    let rows = candidates
        .iter()
        .take(20)
        .filter_map(|index| {
            let node = state.graph().node_at(*index)?;
            let files = exposing_handlers(state, *index, None)
                .into_iter()
                .filter_map(|handler| state.graph().node_at(handler))
                .filter_map(node_path)
                .collect::<Vec<_>>();
            Some(json!({
                "id": node.id.as_str(),
                "label": node.label.as_str(),
                "handler_files": files
            }))
        })
        .collect::<Vec<_>>();
    ambiguous_body(query, rows)
}

pub(super) fn ambiguous_handlers(query: Value, files: &BTreeSet<String>) -> Value {
    let rows = files
        .iter()
        .map(|file| json!({"handler_file": file}))
        .collect::<Vec<_>>();
    ambiguous_body(query, rows)
}

fn ambiguous_body(query: Value, candidates: Vec<Value>) -> Value {
    json!({
        "state": "AMBIGUOUS",
        "query": query,
        "candidates": candidates,
        "endpoint": Value::Null,
        "nodes": [],
        "edges": [],
        "source_excerpts": [],
        "truncated": false,
        "omitted_count": 0,
        "has_more": false,
        "completeness": {
            "complete": false,
            "reason": "ambiguous endpoint"
        },
        "precision": "exact_static_endpoint_lookup",
        "dynamic_dispatch": {
            "evaluated": true,
            "scope": "repository_static_and_declared_runtime_evidence",
            "matches": 0,
            "runtime_evidence_present": false
        },
        "source_mutation": "NONE"
    })
}
