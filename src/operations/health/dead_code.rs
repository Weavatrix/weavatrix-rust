use super::entry_points::entry_points;
use super::paths::{path_is_in_scope, requested_path_scope};
use crate::engine::RepositoryState;
use crate::operations::{optional_bool, optional_u64};
use blazingly_json::{Value, json};
use std::collections::{BTreeSet, VecDeque};
use weavatrix_graph::{EdgeKind, GraphView, NodeIndex, NodeKind};

pub(in crate::operations) fn dead_code(
    state: &RepositoryState,
    args: &Value,
) -> Result<Value, String> {
    let top = usize::try_from(optional_u64(args, "top_n")?.unwrap_or(30))
        .map_err(|_| "top_n is too large".to_owned())?;
    let min_confidence = optional_u64(args, "min_confidence")?.unwrap_or(0);
    if min_confidence > 100 {
        return Err("min_confidence must be between 0 and 100".to_owned());
    }
    let kinds = requested_kinds(args)?;
    let path_scope = requested_path_scope(args)?;
    let _ = optional_bool(args, "include_tests")?;
    let _ = optional_bool(args, "include_classified")?;
    let entries = entry_points(state);
    let reachable = reachable_from(state, &entries);
    let candidates = state
        .graph()
        .nodes()
        .iter()
        .enumerate()
        .filter(|(_, node)| {
            matches!(
                node.kind,
                NodeKind::File
                    | NodeKind::Function
                    | NodeKind::Method
                    | NodeKind::Struct
                    | NodeKind::Enum
                    | NodeKind::Trait
            )
        })
        .filter(|(_, node)| {
            kinds
                .as_ref()
                .is_none_or(|requested| requested.contains(node.kind.as_str()))
        })
        .filter(|(_, node)| {
            path_is_in_scope(
                crate::operations::node_path(node).unwrap_or_default(),
                path_scope.as_deref(),
            )
        })
        .filter(|(slot, _)| crate::operations::node_is_visible(state, *slot, args))
        .filter_map(|(slot, node)| {
            let index = NodeIndex::new(u32::try_from(slot).unwrap_or(u32::MAX));
            if reachable.contains(&index) {
                return None;
            }
            let references = state
                .graph()
                .incoming_at(index)
                .filter(|edge| edge.kind != EdgeKind::Contains)
                .count();
            let (confidence, confidence_score) = confidence(&node.kind);
            if references != 0 || confidence_score < min_confidence {
                return None;
            }
            Some(json!({
                "node": node,
                "confidence": confidence,
                "confidence_score": confidence_score,
                "reason": "unreachable from any declared entry point and no incoming call/import/reference evidence",
                "caveat": "framework, reflection, public API, runtime and generated use may be invisible"
            }))
        })
        .take(top)
        .collect::<Vec<_>>();
    Ok(json!({
        "candidates": candidates,
        "entry_points": entries.iter().filter_map(|index| {
            state.graph().node_at(*index).map(|node| node.id.as_str())
        }).collect::<Vec<_>>(),
        "reachable_nodes": reachable.len(),
        "verdict": "REVIEW_ONLY"
    }))
}

fn requested_kinds(args: &Value) -> Result<Option<BTreeSet<String>>, String> {
    let Some(value) = args.get("kinds") else {
        return Ok(None);
    };
    let Some(items) = value.as_array() else {
        return Err("kinds must be an array of node-kind strings".to_owned());
    };
    let mut kinds = BTreeSet::new();
    for item in items {
        let Some(kind) = item.as_str() else {
            return Err("kinds must be an array of node-kind strings".to_owned());
        };
        kinds.insert(kind.to_ascii_lowercase());
    }
    Ok(Some(kinds))
}

const fn confidence(kind: &NodeKind) -> (&'static str, u64) {
    if matches!(kind, NodeKind::File) {
        ("low", 25)
    } else {
        ("medium", 50)
    }
}

/// Everything a declared entry point can reach through runtime graph edges.
fn reachable_from(state: &RepositoryState, entries: &[NodeIndex]) -> BTreeSet<NodeIndex> {
    let mut seen = entries.iter().copied().collect::<BTreeSet<_>>();
    let mut queue = entries.iter().copied().collect::<VecDeque<_>>();
    while let Some(index) = queue.pop_front() {
        for edge in state.graph().outgoing_edges(index) {
            let Some(kind) = state.graph().edge_at(edge).map(|edge| edge.kind.clone()) else {
                continue;
            };
            if !matches!(
                kind,
                EdgeKind::Contains | EdgeKind::Imports | EdgeKind::ReExports | EdgeKind::Calls
            ) {
                continue;
            }
            let Some(endpoints) = state.graph().edge_endpoints(edge) else {
                continue;
            };
            if seen.insert(endpoints.target()) {
                queue.push_back(endpoints.target());
            }
        }
    }
    seen
}
