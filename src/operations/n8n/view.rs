use crate::engine::RepositoryState;
use blazingly_json::{Value, json};
use weavatrix_graph::{EdgeKind, Node};

pub(super) fn workflow_row(state: &RepositoryState, node: &Node) -> Value {
    let domains = owned_domains(state, node.id.as_str());
    json!({
        "id": node.id,
        "label": node.label,
        "span": node.span,
        "completeness": domain_name(&domains, "completeness:").unwrap_or_else(|| "unspecified".to_owned()),
        "entries": domains.iter().filter(|item| {
            item["name"].as_str().is_some_and(|name| name.starts_with("entry:"))
        }).cloned().collect::<Vec<_>>(),
        "nodes": child_nodes(state, node.id.as_str(), "n8n.node")
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
                "type": domain_name(&domains, "type:"),
                "semantics": domain_name(&domains, "semantics:")
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
                "name": node.label,
                "kind": node.kind,
                "relation": edge.kind
            }))
        })
        .collect()
}

pub(super) fn coverage_from(state: &RepositoryState, path: Option<&str>) -> Value {
    let mut structure = (0, 0);
    let mut semantics = (0, 0);
    let mut expressions = (0, 0);
    for node in state.graph().nodes() {
        if path.is_some_and(|filter| !in_path(node, filter)) {
            continue;
        }
        if let Some(label) = node.label.strip_prefix("coverage:structure:") {
            structure = parse_ratio(label);
        } else if let Some(label) = node.label.strip_prefix("coverage:nodeSemantics:") {
            semantics = parse_ratio(label);
        } else if let Some(label) = node.label.strip_prefix("coverage:expressions:") {
            expressions = parse_ratio(label);
        }
    }
    json!({
        "structure": {"accepted": structure.0, "seen": structure.1},
        "nodeSemantics": {"supported": semantics.0, "seen": semantics.1},
        "expressions": {"resolved": expressions.0, "seen": expressions.1},
        "runtime": {"provided": false}
    })
}

fn parse_ratio(label: &str) -> (u64, u64) {
    let Some((left, right)) = label.split_once('/') else {
        return (0, 0);
    };
    (left.parse().unwrap_or(0), right.parse().unwrap_or(0))
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
    let lower = name.to_ascii_lowercase();
    lower.contains("password")
        || lower.contains("token")
        || lower.contains("secret")
        || lower.contains("redacted")
        || lower.contains("authorization")
        || lower.contains("://") && lower.contains('@')
}
