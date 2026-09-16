use super::detect::is_links_file;
use super::model::binding_kind;
use super::span::span_for;
use crate::language::{DomainFact, FileFacts, SymbolFact, SymbolLocator};
use blazingly_json::Value;
use weavatrix_graph::{EdgeKind, NodeKind};

#[must_use]
pub(crate) fn analyze_links(path: &str, raw: &str, value: &Value) -> Option<FileFacts> {
    if !looks_like(path, value) {
        return None;
    }
    let links = value.get("links")?.as_array()?;
    let mut facts = FileFacts::default();
    for (index, link) in links.iter().enumerate() {
        let diagram = link.get("diagram").and_then(Value::as_str).unwrap_or("");
        let element = link.get("element").and_then(Value::as_str).unwrap_or("");
        if diagram.is_empty() || element.is_empty() {
            continue;
        }
        let span = span_for(path, raw, 0, raw.len().max(1));
        let name = format!("{diagram}#{element}");
        let symbol = SymbolFact {
            name: name.clone(),
            kind: binding_kind(),
            span: span.clone(),
            test_only: false,
            exported: false,
            source_fingerprint: None,
            source_extent: None,
            owner: None,
        };
        let owner = SymbolLocator {
            name: symbol.name.clone(),
            kind: symbol.kind.clone(),
            span: span.clone(),
        };
        facts.symbols.push(symbol);
        for (prefix, key) in [
            ("diagram", "diagram"),
            ("element", "element"),
            ("kind", "kind"),
            ("selector", "selector"),
            ("artifact", "artifact"),
            ("id", "id"),
        ] {
            if let Some(value) = link.get(key).and_then(Value::as_str).filter(|value| !value.is_empty())
            {
                facts.domains.push(DomainFact {
                    name: format!("{prefix}:{value}"),
                    kind: NodeKind::Unknown,
                    relation: EdgeKind::Configures,
                    span: span.clone(),
                    owner: Some(owner.clone()),
                });
            }
        }
        facts.domains.push(DomainFact {
            name: format!("declared:{index}"),
            kind: NodeKind::Unknown,
            relation: EdgeKind::Configures,
            span,
            owner: Some(owner),
        });
    }
    Some(facts)
}

fn looks_like(path: &str, value: &Value) -> bool {
    if is_links_file(path) {
        return true;
    }
    value
        .get("$schema")
        .and_then(Value::as_str)
        .is_some_and(|schema| schema.contains("diagram-links"))
}
