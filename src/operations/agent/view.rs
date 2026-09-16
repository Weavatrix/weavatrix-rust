use crate::engine::RepositoryState;
use crate::operations::source;
use blazingly_json::{Value, json};
use weavatrix_graph::{EdgeKind, Node};

pub(super) fn nodes<'a>(
    state: &'a RepositoryState,
    kind: &str,
    path: Option<&str>,
) -> Vec<&'a Node> {
    state
        .graph()
        .nodes()
        .iter()
        .filter(|node| node.kind.as_str() == kind)
        .filter(|node| path.is_none_or(|filter| in_path(node, filter)))
        .collect()
}

pub(super) fn in_path(node: &Node, filter: &str) -> bool {
    node.span
        .as_ref()
        .is_some_and(|span| span.file == filter || span.file.contains(filter))
}

pub(super) fn row(state: &RepositoryState, node: &Node) -> Value {
    let domains = owned_domains(state, node.id.as_str());
    json!({
        "id": node.id,
        "label": node.label,
        "kind": node.kind,
        "span": node.span,
        "profile": domain_name(&domains, "profile:"),
        "package": domain_name(&domains, "package:"),
        "completeness": domain_name(&domains, "completeness:"),
        "schema": domain_name(&domains, "schema:"),
        "transport": domain_name(&domains, "transport:"),
        "authorization": domain_name(&domains, "authorization:").unwrap_or_else(|| "none".to_owned()),
        "relations": domains
    })
}

pub(super) fn selected(node: &Node) -> Value {
    json!({
        "id": node.id,
        "label": node.label,
        "kind": node.kind,
        "span": node.span
    })
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

pub(super) fn children(state: &RepositoryState, owner: &str) -> Vec<Value> {
    state
        .graph()
        .edges()
        .iter()
        .filter(|edge| edge.source.as_str() == owner && edge.kind == EdgeKind::Contains)
        .filter_map(|edge| {
            let node = state.graph().node(edge.target.as_str())?;
            Some(selected(node))
        })
        .collect()
}

pub(super) fn coverage(state: &RepositoryState, path: Option<&str>) -> Value {
    json!({
        "plugins": nodes(state, "agent.plugin", path).len(),
        "skills": nodes(state, "agent.skill", path).len(),
        "mcp_servers": nodes(state, "agent.mcp_server", path).len(),
        "catalogs": nodes(state, "agent.catalog", path).len(),
        "tools": nodes(state, "agent.tool", path).len(),
        "observations": nodes(state, "agent.observation", path).len(),
        "runtime": {"provided": false},
        "executed": false,
        "authorization": "none"
    })
}

pub(super) fn fragments(state: &RepositoryState, node: &Node) -> Vec<Value> {
    if node.span.is_none() {
        return Vec::new();
    }
    let Ok(source) = source::read_source(
        state,
        &json!({
            "label": node.id.as_str(),
            "before": 1,
            "after": 16
        }),
    ) else {
        return Vec::new();
    };
    vec![json!({
        "path": source["path"],
        "start_line": source["start_line"],
        "lines": source["lines"]
    })]
}

fn domain_name(domains: &[Value], prefix: &str) -> Option<String> {
    domains.iter().find_map(|item| {
        item["name"]
            .as_str()
            .and_then(|name| name.strip_prefix(prefix))
            .map(ToOwned::to_owned)
    })
}
