use super::{neighbors, pagination::page_offset};
use crate::engine::RepositoryState;
use crate::operations::{arg_bool, arg_str, arg_u64, optional_bool, optional_u64};
use blazingly_json::{Value, json};
use std::collections::BTreeMap;
use weavatrix_graph::NodeKind;

pub fn communities(state: &RepositoryState, args: &Value) -> Result<Value, String> {
    let components = state.coupled_components();
    if let Ok(id) = arg_u64(args, "community_id") {
        let id = usize::try_from(id).map_err(|_| "community_id is too large")?;
        let component = components
            .get(id)
            .ok_or_else(|| format!("community not found: {id}"))?;
        let offset = page_offset(args)?;
        if offset > component.len() {
            return Err("cursor offset is outside the selected community".to_owned());
        }
        let max_nodes = usize::try_from(arg_u64(args, "max_nodes").unwrap_or(50)).unwrap_or(50);
        if max_nodes == 0 || max_nodes > 500 {
            return Err("max_nodes must be between 1 and 500".to_owned());
        }
        let end = offset.saturating_add(max_nodes).min(component.len());
        let nodes = component[offset..end]
            .iter()
            .filter_map(|index| state.graph().node_at(*index))
            .collect::<Vec<_>>();
        let returned = nodes.len();
        return Ok(json!({
            "community_id": id,
            "nodes": nodes,
            "page": {
                "offset": offset,
                "returned": returned,
                "total": component.len(),
                "has_more": end < component.len(),
                "next_cursor": (end < component.len()).then(|| format!("v1:{end}"))
            }
        }));
    }
    let top = usize::try_from(arg_u64(args, "top_n").unwrap_or(20)).unwrap_or(20);
    Ok(
        json!({"communities": components.iter().take(top).enumerate().map(|(id, nodes)| {
            json!({"community_id": id, "nodes": nodes.len(), "sample": nodes.iter().take(5)
                .filter_map(|index| state.graph().node_at(*index).map(|node| &node.label))
                .collect::<Vec<_>>()})
        }).collect::<Vec<_>>()}),
    )
}

pub fn module_map(state: &RepositoryState, args: &Value) -> Result<Value, String> {
    let top = usize::try_from(optional_u64(args, "top_n")?.unwrap_or(25))
        .map_err(|_| "top_n is too large")?;
    let depth = usize::try_from(optional_u64(args, "depth")?.unwrap_or(1))
        .map_err(|_| "depth is too large")?;
    if !(1..=8).contains(&depth) {
        return Err("depth must be between 1 and 8".to_owned());
    }
    let include_non_product = optional_bool(args, "include_non_product")?.unwrap_or(false);
    let mut modules = BTreeMap::<String, (u64, u64)>::new();
    for node in state.graph().nodes() {
        let Some(path) = node
            .span
            .as_ref()
            .map(|span| &span.file)
            .or_else(|| (node.kind == NodeKind::File).then_some(&node.label))
        else {
            continue;
        };
        if !include_non_product && crate::operations::health::is_non_product(path) {
            continue;
        }
        // The final segment is the file itself; a module is its directory
        // chain cut at the requested depth.
        let segments = path.split('/').collect::<Vec<_>>();
        let directories = segments.len().saturating_sub(1);
        let module = if directories == 0 {
            "(root)".to_owned()
        } else {
            segments[..directories.min(depth)].join("/")
        };
        let entry = modules.entry(module).or_default();
        if node.kind == NodeKind::File {
            entry.0 += 1;
        } else {
            entry.1 += 1;
        }
    }
    let mut modules = modules.into_iter().collect::<Vec<_>>();
    modules.sort_unstable_by(|left, right| {
        (right.1.0 + right.1.1)
            .cmp(&(left.1.0 + left.1.1))
            .then_with(|| left.0.cmp(&right.0))
    });
    Ok(
        json!({"modules": modules.into_iter().take(top).map(|(path, (files, symbols))| {
        json!({"path": path, "files": files, "symbols": symbols})
    }).collect::<Vec<_>>() }),
    )
}

pub fn endpoints(state: &RepositoryState, args: &Value) -> Result<Value, String> {
    let method = arg_str(args, "method").ok();
    let path = arg_str(args, "path").ok();
    let mut endpoints = state
        .graph()
        .nodes()
        .iter()
        .enumerate()
        .filter(|(_, node)| node.kind == NodeKind::Endpoint)
        .filter(|(_, node)| method.is_none_or(|value| node.label.starts_with(value)))
        .filter(|(_, node)| path.is_none_or(|value| node.label.ends_with(value)))
        .filter(|(slot, _)| crate::operations::node_is_visible(state, *slot, args))
        .map(|(_, node)| node)
        .collect::<Vec<_>>();
    endpoints.sort_unstable_by(|left, right| left.label.cmp(&right.label));
    if let Some(path) = path
        && arg_bool(args, "trace").unwrap_or(false)
    {
        let endpoint = endpoints
            .first()
            .ok_or_else(|| format!("endpoint not found: {path}"))?;
        return neighbors(state, &json!({"label": endpoint.id.as_str()}));
    }
    Ok(json!({"endpoints": endpoints}))
}
