use super::model::{AppRecord, DomainRecord, node_key};
use crate::language::yaml_doc::{self, Node};
use weavatrix_graph::{EdgeKind, NodeKind};
use weavatrix_parse::{Language, extract};

pub(super) fn collect(record: &mut AppRecord, nodes: &[Node], path: &str, raw: &str) {
    for node in nodes {
        let type_name = node
            .get("data")
            .and_then(|data| data.get("type"))
            .and_then(Node::as_str);
        if type_name != Some("code") {
            continue;
        }
        let Some(id) = node.get("id").and_then(Node::as_str) else {
            continue;
        };
        let owner = node_key(&record.key, id);
        let data = node.get("data");
        let Some(code) = data
            .and_then(|data| data.get("code"))
            .and_then(Node::as_str)
        else {
            continue;
        };
        let language = match data
            .and_then(|data| data.get("code_language"))
            .and_then(Node::as_str)
        {
            Some("python3" | "python") => Language::Python,
            _ => Language::JavaScript,
        };
        let facts = extract(code, language);
        let span = data.and_then(|data| data.get("code")).map_or_else(
            || record.span.clone(),
            |node| yaml_doc::span_for(path, raw, node.range().0, node.range().1),
        );
        for declaration in facts.declarations {
            record.domains.push(DomainRecord {
                owner: owner.clone(),
                name: format!("code:decl:{}", declaration.name),
                kind: NodeKind::Function,
                relation: EdgeKind::Configures,
                span: span.clone(),
            });
        }
        for import in facts.imports {
            record.domains.push(DomainRecord {
                owner: owner.clone(),
                name: format!("code:import:{}", import.specifier),
                kind: NodeKind::Module,
                relation: EdgeKind::Imports,
                span: span.clone(),
            });
        }
        for reference in facts.references {
            record.domains.push(DomainRecord {
                owner: owner.clone(),
                name: format!("code:ref:{}", reference.name),
                kind: NodeKind::Unknown,
                relation: EdgeKind::References,
                span: span.clone(),
            });
        }
        if let Some(outputs) = data.and_then(|data| data.get("outputs"))
            && let Some(entries) = outputs.entries()
        {
            for (name, _) in entries {
                record.domains.push(DomainRecord {
                    owner: owner.clone(),
                    name: format!("output:{}", name.decoded),
                    kind: NodeKind::Column,
                    relation: EdgeKind::Configures,
                    span: span.clone(),
                });
            }
        }
    }
}
