use super::super::neighbors;
use crate::engine::RepositoryState;
use crate::operations::{arg_bool, arg_str};
use blazingly_json::{Value, json};
use weavatrix_graph::NodeKind;

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
