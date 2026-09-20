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

pub fn stats(state: &RepositoryState, args: &Value) -> Result<Value, String> {
    let census = state.census();
    // Static per build, not per repository or per call: opt in rather than pay for it every time.
    let capabilities = if optional_bool(args, "include_capabilities")?.unwrap_or(false) {
        json!(state.snapshot().capabilities)
    } else {
        Value::Null
    };
    Ok(json!({
        "repository": state.snapshot().repository,
        "revision": state.snapshot().revision,
        "nodes": state.graph().node_count(),
        "edges": state.graph().edge_count(),
        "build_ms": state.build_time().as_secs_f64() * 1000.0,
        "node_kinds": census.kinds,
        "relations": census.relations,
        "evidence": census.evidence,
        "capabilities": capabilities,
        "freshness": {
            "state": "CURRENT",
            "source_revision": state.snapshot().revision,
            "incremental_hashes_reused": state.scan_report().cache.reused_hashes,
            "content_reads": state.scan_report().cache.content_reads
        }
    }))
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
            if total >= offset && items.len() < max_results {
                let other = if direction == "outgoing" {
                    state.graph().node(edge.target.as_str())
                } else {
                    state.graph().node(edge.source.as_str())
                };
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
    Ok(json!({
        "node": state.node(index)?,
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
        .filter(|(_, node)| !matches!(node.kind, NodeKind::Repository | NodeKind::Package))
        .filter(|(slot, _)| crate::operations::node_is_visible(state, *slot, args))
        .map(|(slot, node)| {
            let index = NodeIndex::new(u32::try_from(slot).unwrap_or(u32::MAX));
            let incoming = state.graph().in_degree(index).unwrap_or(0);
            let outgoing = state.graph().out_degree(index).unwrap_or(0);
            (incoming + outgoing, incoming, outgoing, node)
        })
        .collect::<Vec<_>>();
    nodes.sort_unstable_by(|left, right| {
        right
            .0
            .cmp(&left.0)
            .then_with(|| left.3.id.cmp(&right.3.id))
    });
    json!({
        "hubs": nodes.into_iter().take(top).map(|(degree, incoming, outgoing, node)| {
            json!({"node": node, "degree": degree, "incoming": incoming, "outgoing": outgoing})
        }).collect::<Vec<_>>()
    })
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
