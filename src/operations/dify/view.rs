use crate::engine::RepositoryState;
use blazingly_json::{Value, json};
use weavatrix_graph::{EdgeKind, Node};

pub(super) fn app_row(state: &RepositoryState, node: &Node) -> Value {
    let domains = owned_domains(state, node.id.as_str());
    json!({
        "id": node.id,
        "label": node.label,
        "span": node.span,
        "mode": domain_name(&domains, "mode:").unwrap_or_else(|| "unspecified".to_owned()),
        "dsl": domain_name(&domains, "dsl:").unwrap_or_else(|| "unspecified".to_owned()),
        "nodes": child_nodes(state, node.id.as_str(), "dify.node")
    })
}

pub(super) fn in_path(node: &Node, filter: &str) -> bool {
    node.span
        .as_ref()
        .is_some_and(|span| span.file == filter || span.file.contains(filter))
}

fn child_nodes(state: &RepositoryState, parent: &str, kind: &str) -> Vec<Value> {
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
                "kind": node.kind,
                "span": node.span,
                "type": domain_name(&domains, "type:")
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
            if secret_label(node.label.as_str()) {
                return None;
            }
            Some(json!({
                "id": node.id,
                "name": node.label,
                "kind": node.kind,
                "relation": edge.kind,
                "span": node.span,
                "target": edge.target
            }))
        })
        .collect()
}

pub(super) fn coverage_from(state: &RepositoryState, path: Option<&str>) -> Value {
    let mut structure = (0, 0);
    let mut semantics = (0, 0);
    let mut variables = (0, 0);
    for owner in state.graph().nodes().iter().filter(|node| {
        node.kind.as_str() == "dify.app" && path.is_none_or(|filter| in_path(node, filter))
    }) {
        for item in owned_domains(state, owner.id.as_str()) {
            let Some(name) = item["name"].as_str() else {
                continue;
            };
            if let Some(label) = name.strip_prefix("coverage:structure:") {
                add_ratio(&mut structure, label);
            } else if let Some(label) = name.strip_prefix("coverage:nodeSemantics:") {
                add_ratio(&mut semantics, label);
            } else if let Some(label) = name.strip_prefix("coverage:variables:") {
                add_ratio(&mut variables, label);
            }
        }
    }
    json!({
        "structure": {"accepted": structure.0, "seen": structure.1},
        "nodeSemantics": {"supported": semantics.0, "seen": semantics.1},
        "variables": {"resolved": variables.0, "seen": variables.1},
        "runtime": {"provided": false}
    })
}

fn add_ratio(slot: &mut (u64, u64), label: &str) {
    let Some((left, right)) = label.split_once('/') else {
        return;
    };
    slot.0 += left.parse().unwrap_or(0);
    slot.1 += right.parse().unwrap_or(0);
}

fn domain_name(domains: &[Value], prefix: &str) -> Option<String> {
    domains.iter().find_map(|item| {
        item["name"]
            .as_str()
            .and_then(|name| name.strip_prefix(prefix))
            .map(str::to_owned)
    })
}

pub(super) fn secret_domain(item: &Value) -> bool {
    item["name"].as_str().is_some_and(secret_label)
}

fn secret_label(name: &str) -> bool {
    crate::language::dify_secret_label(name)
}
