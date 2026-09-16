use super::model::{AppRecord, DomainRecord, LinkRecord, flows_to, node_key, port_kind};
use crate::language::yaml_doc::{self, Node};
use weavatrix_graph::{EdgeKind, NodeKind};

pub(super) fn collect(record: &mut AppRecord, edges: &[Node], path: &str, raw: &str) {
    for edge in edges {
        let Some(source) = edge.get("source").and_then(Node::as_str) else {
            continue;
        };
        let Some(target) = edge.get("target").and_then(Node::as_str) else {
            continue;
        };
        let source_port = edge
            .get("sourceHandle")
            .and_then(Node::as_str)
            .unwrap_or("source");
        let target_port = edge
            .get("targetHandle")
            .and_then(Node::as_str)
            .unwrap_or("target");
        let from = node_key(&record.key, source);
        let to = node_key(&record.key, target);
        let (start, end) = edge.range();
        let span = yaml_doc::span_for(path, raw, start, end);
        record.domains.push(DomainRecord {
            owner: from.clone(),
            name: format!("port:out:{source_port}"),
            kind: port_kind(),
            relation: EdgeKind::Contains,
            span: span.clone(),
        });
        record.domains.push(DomainRecord {
            owner: to.clone(),
            name: format!("port:in:{target_port}"),
            kind: port_kind(),
            relation: EdgeKind::Contains,
            span: span.clone(),
        });
        record.links.push(LinkRecord {
            from,
            to,
            kind: flows_to(),
            span: span.clone(),
            detail: format!("{source_port}->{target_port}"),
        });
        if edge
            .get("data")
            .and_then(|data| data.get("isInIteration"))
            .and_then(Node::as_bool)
            == Some(true)
            && let Some(iteration) = edge
                .get("data")
                .and_then(|data| data.get("iteration_id"))
                .and_then(Node::as_str)
        {
            record.domains.push(DomainRecord {
                owner: node_key(&record.key, source),
                name: format!("scope:iteration:{iteration}"),
                kind: NodeKind::Unknown,
                relation: EdgeKind::Contains,
                span,
            });
        }
    }
    for node in &record.nodes {
        if let Some(parent) = &node.parent {
            record.domains.push(DomainRecord {
                owner: node.key.clone(),
                name: format!("parent:{parent}"),
                kind: NodeKind::Unknown,
                relation: EdgeKind::Contains,
                span: node.span.clone(),
            });
        }
    }
}
