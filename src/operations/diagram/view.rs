use crate::engine::RepositoryState;
use blazingly_json::{Value, json};
use weavatrix_graph::{EdgeKind, Node};

pub(super) fn diagram_row(state: &RepositoryState, node: &Node) -> Value {
    let domains = owned_domains(state, node.id.as_str());
    json!({
        "id": node.id,
        "label": node.label,
        "span": node.span,
        "kind": domain_name(&domains, "kind:").unwrap_or_else(|| "flowchart".to_owned()),
        "direction": domain_name(&domains, "direction:").unwrap_or_else(|| "unspecified".to_owned()),
        "completeness": domain_name(&domains, "completeness:").unwrap_or_else(|| "partial".to_owned()),
        "elements": children(state, node.id.as_str(), "mermaid.element"),
        "bound": binding_count(state, node.label.as_str())
    })
}

pub(super) fn in_path(node: &Node, filter: &str) -> bool {
    node.span
        .as_ref()
        .is_some_and(|span| span.file == filter || span.file.contains(filter))
}

pub(super) fn children(state: &RepositoryState, parent: &str, kind: &str) -> Vec<Value> {
    state
        .graph()
        .edges()
        .iter()
        .filter(|edge| edge.source.as_str() == parent && edge.kind == EdgeKind::Contains)
        .filter_map(|edge| state.graph().node(edge.target.as_str()))
        .filter(|node| node.kind.as_str() == kind)
        .map(|node| {
            let domains = owned_domains(state, node.id.as_str());
            json!({
                "id": node.id,
                "label": node.label,
                "display": domain_name(&domains, "label:").unwrap_or_else(|| node.label.clone()),
                "kind": node.kind,
                "span": node.span
            })
        })
        .collect()
}

pub(super) fn owned_domains(state: &RepositoryState, owner: &str) -> Vec<Value> {
    state
        .graph()
        .edges()
        .iter()
        .filter(|edge| edge.source.as_str() == owner)
        .filter_map(|edge| {
            let node = state.graph().node(edge.target.as_str())?;
            Some(json!({
                "id": node.id,
                "name": node.label,
                "kind": node.kind,
                "relation": edge.kind,
                "span": node.span
            }))
        })
        .collect()
}

pub(super) fn domain_name(domains: &[Value], prefix: &str) -> Option<String> {
    domains.iter().find_map(|item| {
        item["name"]
            .as_str()
            .and_then(|name| name.strip_prefix(prefix))
            .map(ToOwned::to_owned)
    })
}

fn binding_count(state: &RepositoryState, diagram: &str) -> usize {
    state
        .graph()
        .nodes()
        .iter()
        .filter(|node| node.kind.as_str() == "mermaid.binding")
        .filter(|node| {
            owned_domains(state, node.id.as_str())
                .iter()
                .any(|item| item["name"].as_str() == Some(&format!("diagram:{diagram}")))
        })
        .count()
}
