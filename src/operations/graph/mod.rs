use crate::engine::RepositoryState;
use crate::operations::{arg_str, arg_u64, optional_bool};
use blazingly_json::{Value, json};
use weavatrix_graph::{NodeIndex, NodeKind};

mod bounds;
mod coupling;
mod pagination;
mod path;
mod query;
mod related;
mod seeds;
mod trace;
mod views;
mod walk;

pub(crate) use coupling::coupling_relations;
pub(crate) use pagination::page_offset;
pub use path::path;
pub use query::{dependents, query};
pub(crate) use related::pick_related;
pub(super) use trace::endpoint as trace_endpoint;
pub use views::{communities, endpoints, module_map};
pub(super) use walk::traverse;

pub(super) fn counts(state: &RepositoryState) -> Value {
    json!({
        "nodes": state.graph().node_count(),
        "edges": state.graph().edge_count(),
        "build_ms": state.build_time().as_secs_f64() * 1000.0
    })
}

pub(super) fn compact_node(node: &weavatrix_graph::Node) -> Value {
    let mut value = json!({
        "id": node.id,
        "label": node.label,
        "kind": node.kind
    });
    if let Some(object) = value.as_object_mut() {
        if let Some(language) = &node.language {
            object.insert("language".to_owned(), json!(language));
        }
        if let Some(span) = &node.span {
            object.insert("span".to_owned(), json!(span));
        }
    }
    value
}

pub fn stats(state: &RepositoryState, args: &Value) -> Result<Value, String> {
    let census = state.census();
    let fresh = if state.evidence().receipt().complete {
        "CURRENT"
    } else {
        "INCOMPLETE"
    };
    let share = evidence_share(&census.evidence);
    // Static per build, not per repository or per call: opt in rather than pay for it every time.
    // evidence counts edges. CURRENT means this loaded snapshot, not a new walk of the disk.
    let mut report = json!({
        "repository": state.snapshot().repository,
        "revision": state.snapshot().revision,
        "nodes": state.graph().node_count(),
        "edges": state.graph().edge_count(),
        "build_ms": state.build_time().as_secs_f64() * 1000.0,
        "node_kinds": census.kinds,
        "relations": census.relations,
        "evidence": census.evidence,
        "evidence_unit": "edges",
        "evidence_share": share,
        "freshness": {
            "state": fresh,
            "basis": "loaded_snapshot",
            "source_revision": state.snapshot().revision,
            "incremental_hashes_reused": state.scan_report().cache.reused_hashes,
            "content_reads": state.scan_report().cache.content_reads
        }
    });
    if optional_bool(args, "include_capabilities")?.unwrap_or(false)
        && let Some(object) = report.as_object_mut()
    {
        object.insert(
            "capabilities".to_owned(),
            json!(state.snapshot().capabilities),
        );
    }
    Ok(report)
}

pub fn get_node(state: &RepositoryState, args: &Value) -> Result<Value, String> {
    let index = state.resolve_node(arg_str(args, "label")?)?;
    let node = state.node(index)?;
    Ok(json!({
        "node": node,
        "incoming": state.graph().in_degree(index),
        "outgoing": state.graph().out_degree(index)
    }))
}

pub fn neighbors(state: &RepositoryState, args: &Value) -> Result<Value, String> {
    let index = state.resolve_node(arg_str(args, "label")?)?;
    let filter = relation_filter(args)?;
    let offset = page_offset(args)?;
    let max_results = usize::try_from(arg_u64(args, "max_results").unwrap_or(50)).unwrap_or(50);
    if max_results == 0 || max_results > 500 {
        return Err("max_results must be between 1 and 500".to_owned());
    }
    let full = arg_str(args, "response_detail").unwrap_or("compact") == "full";
    let mut items = Vec::with_capacity(max_results);
    let mut total = 0_usize;
    for (direction, edges) in [
        (
            "outgoing",
            state.graph().outgoing_at(index).collect::<Vec<_>>(),
        ),
        (
            "incoming",
            state.graph().incoming_at(index).collect::<Vec<_>>(),
        ),
    ] {
        for edge in edges {
            if filter
                .as_ref()
                .is_some_and(|kinds| !kinds.contains(edge.kind.as_str()))
            {
                continue;
            }
            let other = if direction == "outgoing" {
                state.graph().node(edge.target.as_str())
            } else {
                state.graph().node(edge.source.as_str())
            };
            if let Some(node) = other
                && let Some(slot) = state.graph().node_index(node.id.as_str())
                && !crate::operations::node_is_visible(state, slot.index(), args)
            {
                continue;
            }
            if total >= offset && items.len() < max_results {
                items.push(if full {
                    json!({"direction": direction, "edge": edge, "node": other})
                } else {
                    json!({
                        "direction": direction,
                        "relation": edge.kind,
                        "provenance": edge.provenance,
                        "node": other.map(|node| json!({
                            "id": node.id,
                            "label": node.label,
                            "kind": node.kind,
                            "span": node.span
                        }))
                    })
                });
            }
            total = total.saturating_add(1);
        }
    }
    let returned = items.len();
    let end = offset.saturating_add(returned);
    let seed = state.node(index)?;
    Ok(json!({
        "node": if full { json!(seed) } else { compact_node(seed) },
        "neighbors": items,
        "page": {
            "offset": offset,
            "returned": returned,
            "total": total,
            "has_more": end < total,
            "next_cursor": (end < total).then(|| format!("v1:{end}"))
        }
    }))
}

pub fn hubs(state: &RepositoryState, args: &Value) -> Value {
    let top = usize::try_from(arg_u64(args, "top_n").unwrap_or(10)).unwrap_or(10);
    let mut nodes = state
        .graph()
        .nodes()
        .iter()
        .enumerate()
        .filter(|(_, node)| hub_candidate(node))
        .filter(|(slot, _)| crate::operations::node_is_visible(state, *slot, args))
        .map(|(slot, node)| {
            let index = NodeIndex::new(u32::try_from(slot).unwrap_or(u32::MAX));
            let incoming = state.graph().in_degree(index).unwrap_or(0);
            let outgoing = state.graph().out_degree(index).unwrap_or(0);
            let outside = outside_files(state, index, node);
            (outside, incoming + outgoing, incoming, outgoing, node)
        })
        .collect::<Vec<_>>();
    nodes.sort_unstable_by(|left, right| {
        right
            .0
            .cmp(&left.0)
            .then_with(|| right.1.cmp(&left.1))
            .then_with(|| left.4.id.cmp(&right.4.id))
    });
    json!({
        "hubs": nodes.into_iter().take(top).map(|(_outside, degree, incoming, outgoing, node)| {
            json!({"node": compact_node(node), "degree": degree, "incoming": incoming, "outgoing": outgoing})
        }).collect::<Vec<_>>()
    })
}

fn evidence_share(evidence: &std::collections::BTreeMap<String, u64>) -> Value {
    let total = evidence.values().copied().sum::<u64>();
    if total == 0 {
        return json!({});
    }
    let mut share = json!({});
    if let Some(object) = share.as_object_mut() {
        for (key, count) in evidence {
            let rounded = ((*count as f64) / (total as f64) * 1000.0).round() / 1000.0;
            object.insert(key.clone(), json!(rounded));
        }
    }
    share
}

const HUB_LABELS: &[&str] = &[
    "String",
    "Log",
    "Error",
    "int",
    "any",
    "Object",
    "Promise",
    "Map",
    "Array",
    "console",
    "number",
    "string",
    "boolean",
];

fn hub_candidate(node: &weavatrix_graph::Node) -> bool {
    if matches!(
        node.kind,
        NodeKind::Repository
            | NodeKind::Package
            | NodeKind::File
            | NodeKind::Module
            | NodeKind::Unknown
            | NodeKind::Constant
            | NodeKind::ConfigKey
    ) {
        return false;
    }
    if HUB_LABELS.contains(&node.label.as_str()) {
        return false;
    }
    let Some(span) = node.span.as_ref() else {
        return true;
    };
    let file = span.file.as_str();
    !(ends_with_ignore(file, ".css")
        || ends_with_ignore(file, ".scss")
        || ends_with_ignore(file, ".less"))
}

fn ends_with_ignore(path: &str, suffix: &str) -> bool {
    path.len() >= suffix.len() && path[path.len() - suffix.len()..].eq_ignore_ascii_case(suffix)
}

fn outside_files(state: &RepositoryState, index: NodeIndex, node: &weavatrix_graph::Node) -> usize {
    let mine = node.span.as_ref().map(|span| span.file.as_str());
    let mut files = std::collections::BTreeSet::new();
    for edge in state.graph().incoming_at(index) {
        let Some(other) = state.graph().node(edge.source.as_str()) else {
            continue;
        };
        let Some(span) = other.span.as_ref() else {
            continue;
        };
        if mine.is_some_and(|file| file == span.file.as_str()) {
            continue;
        }
        files.insert(span.file.as_str());
    }
    files.len()
}

pub(super) fn relation_filter(
    args: &Value,
) -> Result<Option<std::collections::BTreeSet<String>>, String> {
    const EXPECTED: &str = "relation_filter must be a relation name or a non-empty array of them";
    let Some(value) = args.get("relation_filter") else {
        return Ok(None);
    };
    if let Some(value) = value.as_str() {
        return Ok(Some(std::collections::BTreeSet::from([value.to_owned()])));
    }
    let items = value.as_array().ok_or_else(|| EXPECTED.to_owned())?;
    let kinds = items
        .iter()
        .map(|item| item.as_str().map(str::to_owned))
        .collect::<Option<std::collections::BTreeSet<String>>>()
        .ok_or_else(|| EXPECTED.to_owned())?;
    if kinds.is_empty() {
        return Err(EXPECTED.to_owned());
    }
    Ok(Some(kinds))
}
