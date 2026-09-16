use super::model::{AppRecord, DomainRecord, node_key};
use crate::language::yaml_doc::{self, Node};
use weavatrix_graph::NodeKind;

pub(super) fn collect(record: &mut AppRecord, root: &Node, nodes: &[Node], path: &str, raw: &str) {
    if let Some(dependencies) = root.get("dependencies").and_then(Node::items) {
        for item in dependencies {
            let name = item
                .as_str()
                .or_else(|| item.get("name").and_then(Node::as_str))
                .unwrap_or("plugin");
            record.domains.push(domain(
                &record.key,
                format!("plugin:{name}"),
                "imports_plugin",
                record.span.clone(),
            ));
        }
    }
    for node in nodes {
        let Some(id) = node.get("id").and_then(Node::as_str) else {
            continue;
        };
        let owner = node_key(&record.key, id);
        let data = node.get("data");
        if let Some(model) = data.and_then(|data| data.get("model")) {
            if let Some(selector) = model
                .get("model_selector")
                .or_else(|| data.and_then(|data| data.get("model_selector")))
            {
                env_model(record, &owner, selector, path, raw);
            }
            if let Some(name) = model.get("name").and_then(Node::as_str) {
                let provider = model.get("provider").and_then(Node::as_str).unwrap_or("");
                record.domains.push(domain(
                    &owner,
                    format!("model:{provider}:{name}"),
                    "uses_model",
                    span(path, raw, model),
                ));
            }
        }
        if let Some(tools) = data
            .and_then(|data| data.get("tools"))
            .and_then(Node::items)
        {
            for tool in tools {
                let name = tool
                    .get("provider_name")
                    .or_else(|| tool.get("name"))
                    .and_then(Node::as_str)
                    .unwrap_or("tool");
                record.domains.push(domain(
                    &owner,
                    format!("tool:{name}"),
                    "invokes_tool",
                    span(path, raw, tool),
                ));
            }
        }
        if data
            .and_then(|data| data.get("type"))
            .and_then(Node::as_str)
            == Some("knowledge-retrieval")
        {
            record.domains.push(domain(
                &owner,
                "knowledge:configured",
                "queries_knowledge",
                span(path, raw, node),
            ));
        }
    }
}

fn env_model(record: &mut AppRecord, owner: &str, selector: &Node, path: &str, raw: &str) {
    let segments = selector
        .items()
        .map(|items| items.iter().filter_map(Node::as_str).collect::<Vec<_>>())
        .unwrap_or_default();
    if segments.first() == Some(&"env") {
        let name = segments.get(1).copied().unwrap_or("MODEL");
        record.domains.push(domain(
            owner,
            format!("model:env:{name}"),
            "uses_model",
            span(path, raw, selector),
        ));
    }
}

fn domain(
    owner: &str,
    name: impl Into<String>,
    relation: &str,
    span: weavatrix_graph::SourceSpan,
) -> DomainRecord {
    let relation = match relation {
        "uses_model" => super::model::uses_model(),
        "invokes_tool" => super::model::invokes_tool(),
        "queries_knowledge" => super::model::queries_knowledge(),
        "imports_plugin" => super::model::imports_plugin(),
        _ => weavatrix_graph::EdgeKind::Configures,
    };
    DomainRecord {
        owner: owner.to_owned(),
        name: name.into(),
        kind: NodeKind::Unknown,
        relation,
        span,
    }
}

fn span(path: &str, raw: &str, node: &Node) -> weavatrix_graph::SourceSpan {
    let (start, end) = node.range();
    yaml_doc::span_for(path, raw, start, end)
}
