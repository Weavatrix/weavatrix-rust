use crate::engine::RepositoryState;
use blazingly_json::{Value, json};
use weavatrix_graph::Node;

#[must_use]
pub(super) fn in_path(node: &Node, filter: &str) -> bool {
    node.span
        .as_ref()
        .is_some_and(|span| span.file.contains(filter))
        || node.label.contains(filter)
        || node.id.as_str().contains(filter)
}

#[must_use]
pub(super) fn owned_domains<'a>(state: &'a RepositoryState, owner: &str) -> Vec<&'a Node> {
    state
        .graph()
        .edges()
        .iter()
        .filter(|edge| edge.source.as_str() == owner && edge.kind.as_str() == "configures")
        .filter_map(|edge| state.graph().node(edge.target.as_str()))
        .collect()
}

#[must_use]
pub(super) fn domain_names(state: &RepositoryState, owner: &str) -> Vec<String> {
    owned_domains(state, owner)
        .into_iter()
        .map(|node| node.label.clone())
        .collect()
}

#[must_use]
pub(super) fn file_of(node: &Node) -> String {
    node.span
        .as_ref()
        .map(|span| span.file.clone())
        .unwrap_or_default()
}

#[must_use]
pub(super) fn consumers_of(state: &RepositoryState, target: &str) -> Vec<Value> {
    state
        .graph()
        .edges()
        .iter()
        .filter(|edge| edge.target.as_str() == target && edge.kind.as_str() == "web3.binds")
        .filter_map(|edge| {
            let source = state.graph().node(edge.source.as_str())?;
            Some(json!({
                "id": source.id,
                "label": source.label,
                "kind": source.kind,
                "file": file_of(source),
                "binding": binding_status(state, source),
                "span": source.span
            }))
        })
        .collect()
}

#[must_use]
pub(super) fn binding_status(state: &RepositoryState, node: &Node) -> String {
    domain_names(state, node.id.as_str())
        .into_iter()
        .find_map(|name| name.strip_prefix("web3.binding:").map(str::to_owned))
        .unwrap_or_else(|| "unresolved".to_owned())
}

#[must_use]
pub(super) fn coverage(state: &RepositoryState) -> Value {
    let decode = state
        .snapshot()
        .diagnostics
        .iter()
        .any(|item| item.code.starts_with("web3."));
    json!({
        "sourceBinding": if decode { "partial" } else { "supplied" },
        "consumerBindings": "partial",
        "deployment": "not_provided",
        "runtime": { "provided": false }
    })
}
