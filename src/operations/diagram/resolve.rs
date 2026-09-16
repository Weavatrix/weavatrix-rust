use super::view::{domain_name, owned_domains};
use crate::engine::RepositoryState;
use blazingly_json::{Value, json};
use weavatrix_graph::Node;

pub(super) fn binding_row(state: &RepositoryState, node: &Node) -> Value {
    let domains = owned_domains(state, node.id.as_str());
    let selector = domain_name(&domains, "selector:");
    let artifact = domain_name(&domains, "artifact:");
    let native_id = domain_name(&domains, "id:");
    let kind = domain_name(&domains, "kind:").unwrap_or_else(|| "symbol".to_owned());
    let (status, targets) = resolve(state, &kind, selector.as_deref(), artifact.as_deref(), native_id.as_deref());
    json!({
        "id": node.id,
        "label": node.label,
        "diagram": domain_name(&domains, "diagram:"),
        "element": domain_name(&domains, "element:"),
        "kind": kind,
        "selector": selector,
        "artifact": artifact,
        "status": status,
        "targets": targets
    })
}

fn resolve(
    state: &RepositoryState,
    kind: &str,
    selector: Option<&str>,
    artifact: Option<&str>,
    native_id: Option<&str>,
) -> (&'static str, Vec<Value>) {
    let matches = state
        .graph()
        .nodes()
        .iter()
        .filter(|node| node_matches(state, node, kind, selector, artifact, native_id))
        .map(|node| {
            json!({
                "id": node.id,
                "label": node.label,
                "kind": node.kind,
                "span": node.span
            })
        })
        .collect::<Vec<_>>();
    match matches.len() {
        0 => ("missing", matches),
        1 => ("exact", matches),
        _ => ("ambiguous", matches),
    }
}

fn node_matches(
    state: &RepositoryState,
    node: &Node,
    kind: &str,
    selector: Option<&str>,
    artifact: Option<&str>,
    native_id: Option<&str>,
) -> bool {
    if let Some(path) = artifact
        && !node
            .span
            .as_ref()
            .is_some_and(|span| span.file == path || span.file.contains(path))
    {
        return false;
    }
    if kind == "dify.node" || kind == "n8n.node" {
        return node.kind.as_str() == kind
            && native_id.is_some_and(|id| {
                owned_domains(state, node.id.as_str())
                    .iter()
                    .any(|item| item["name"].as_str() == Some(&format!("node_id:{id}")))
            });
    }
    selector.is_some_and(|wanted| node.label == wanted || node.id.as_str().ends_with(wanted))
}
