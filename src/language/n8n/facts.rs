use super::model::{
    DomainBatch, DomainRecord, LinkRecord, WorkflowRecord, node_kind, port_kind, workflow_kind,
};
use crate::language::{
    BoundEdgeFact, DomainFact, FileFacts, ReferenceFact, SymbolFact, SymbolLocator,
};
use std::collections::{BTreeMap, BTreeSet};
use weavatrix_graph::{EdgeKind, NodeKind, SourceSpan};

pub(super) fn to_file_facts(path: &str, batch: &DomainBatch) -> FileFacts {
    let mut facts = FileFacts {
        diagnostics: batch.diagnostics.clone(),
        ..FileFacts::default()
    };
    let mut locators = BTreeMap::<String, SymbolLocator>::new();
    for workflow in &batch.workflows {
        let workflow_symbol = symbol(&workflow.name, workflow_kind(), workflow.span.clone());
        locators.insert(workflow.key.clone(), locator(&workflow_symbol));
        facts.symbols.push(workflow_symbol);
        identity_domains(&mut facts, workflow);
        for node in &workflow.nodes {
            let node_symbol = symbol(&node.name, node_kind(), node.span.clone());
            locators.insert(node.key.clone(), locator(&node_symbol));
            facts.references.push(reference(
                &workflow.name,
                workflow_kind(),
                &workflow.span,
                &node.name,
                EdgeKind::Contains,
                node.span.clone(),
            ));
            facts.symbols.push(node_symbol);
            if !node.id.is_empty() {
                facts.domains.push(DomainFact {
                    name: format!("node_id:{}", node.id),
                    kind: NodeKind::Unknown,
                    relation: EdgeKind::Configures,
                    span: node.span.clone(),
                    owner: Some(SymbolLocator {
                        name: node.name.clone(),
                        kind: node_kind(),
                        span: node.span.clone(),
                    }),
                });
            }
            facts.domains.push(DomainFact {
                name: format!("type:{}@{}", node.type_name, node.type_version),
                kind: NodeKind::TypeAlias,
                relation: EdgeKind::Configures,
                span: node.span.clone(),
                owner: Some(SymbolLocator {
                    name: node.name.clone(),
                    kind: node_kind(),
                    span: node.span.clone(),
                }),
            });
            facts.domains.push(DomainFact {
                name: format!("semantics:{}", node.semantics.as_str()),
                kind: NodeKind::Unknown,
                relation: EdgeKind::Configures,
                span: node.span.clone(),
                owner: Some(SymbolLocator {
                    name: node.name.clone(),
                    kind: node_kind(),
                    span: node.span.clone(),
                }),
            });
        }
        emit_ports(&mut facts, &mut locators, workflow);
        for link in &workflow.links {
            emit_link(&mut facts, &locators, link);
        }
        for domain in &workflow.domains {
            emit_domain(&mut facts, &locators, domain);
        }
        super::coverage::emit(&mut facts, workflow);
    }
    if batch.truncated {
        facts.diagnostics.push(crate::model::Diagnostic {
            code: "n8n.truncated".into(),
            message: "n8n analysis stopped at a declared limit; the result is not complete".into(),
            span: Some(super::locations::file_span(path)),
        });
    }
    let _ = batch.coverage;
    facts
}

fn identity_domains(facts: &mut FileFacts, workflow: &WorkflowRecord) {
    if let Some(id) = &workflow.id {
        facts.domains.push(DomainFact {
            name: format!("workflow_id:{id}"),
            kind: workflow_kind(),
            relation: EdgeKind::Configures,
            span: workflow.span.clone(),
            owner: Some(SymbolLocator {
                name: workflow.name.clone(),
                kind: workflow_kind(),
                span: workflow.span.clone(),
            }),
        });
    }
    facts.domains.push(DomainFact {
        name: format!(
            "completeness:{}",
            match workflow.completeness {
                super::detect::Completeness::Full => "full",
                super::detect::Completeness::Partial => "partial",
            }
        ),
        kind: NodeKind::Unknown,
        relation: EdgeKind::Configures,
        span: workflow.span.clone(),
        owner: Some(SymbolLocator {
            name: workflow.name.clone(),
            kind: workflow_kind(),
            span: workflow.span.clone(),
        }),
    });
}

fn emit_ports(
    facts: &mut FileFacts,
    locators: &mut BTreeMap<String, SymbolLocator>,
    workflow: &WorkflowRecord,
) {
    let mut ports = BTreeSet::<(String, String, SourceSpan)>::new();
    for link in &workflow.links {
        if let Some((node_key, label)) = port_parts(&link.from)
            && let Some(node) = workflow.nodes.iter().find(|node| node.key == node_key)
        {
            ports.insert((link.from.clone(), node.name.clone(), node.span.clone()));
            let _ = label;
        }
        if let Some((node_key, _)) = port_parts(&link.to)
            && let Some(node) = workflow.nodes.iter().find(|node| node.key == node_key)
        {
            ports.insert((link.to.clone(), node.name.clone(), node.span.clone()));
        }
    }
    for (port, node_name, span) in ports {
        let port_symbol = symbol(&port, port_kind(), span.clone());
        locators.insert(port.clone(), locator(&port_symbol));
        facts.references.push(reference(
            &node_name,
            node_kind(),
            &span,
            &port,
            EdgeKind::Contains,
            span.clone(),
        ));
        facts.symbols.push(port_symbol);
    }
}

fn emit_link(facts: &mut FileFacts, locators: &BTreeMap<String, SymbolLocator>, link: &LinkRecord) {
    let Some(from) = locators.get(&link.from) else {
        return;
    };
    let Some(to) = locators.get(&link.to) else {
        return;
    };
    facts.bound_edges.push(BoundEdgeFact {
        from: from.clone(),
        to: to.clone(),
        kind: link.kind.clone(),
        span: link.span.clone(),
        detail: link.detail.clone(),
    });
    if !link.detail.is_empty() {
        facts.domains.push(DomainFact {
            name: format!("link:{}", link.detail),
            kind: NodeKind::Unknown,
            relation: link.kind.clone(),
            span: link.span.clone(),
            owner: Some(from.clone()),
        });
    }
}

fn emit_domain(
    facts: &mut FileFacts,
    locators: &BTreeMap<String, SymbolLocator>,
    domain: &DomainRecord,
) {
    facts.domains.push(DomainFact {
        name: domain.name.clone(),
        kind: domain.kind.clone(),
        relation: domain.relation.clone(),
        span: domain.span.clone(),
        owner: locators.get(&domain.owner).cloned(),
    });
}

fn port_parts(key: &str) -> Option<(String, String)> {
    let (node_key, label) = key.split_once('#')?;
    Some((node_key.to_owned(), label.to_owned()))
}

fn symbol(name: &str, kind: NodeKind, span: SourceSpan) -> SymbolFact {
    SymbolFact {
        name: name.to_owned(),
        kind,
        span,
        test_only: false,
        exported: true,
        source_fingerprint: None,
        source_extent: None,
        owner: None,
    }
}

fn locator(symbol: &SymbolFact) -> SymbolLocator {
    SymbolLocator {
        name: symbol.name.clone(),
        kind: symbol.kind.clone(),
        span: symbol.span.clone(),
    }
}

fn reference(
    owner_name: &str,
    owner_kind: NodeKind,
    owner_span: &SourceSpan,
    name: &str,
    kind: EdgeKind,
    span: SourceSpan,
) -> ReferenceFact {
    ReferenceFact {
        name: name.to_owned(),
        kind,
        receiver: None,
        qualified: false,
        span,
        owner: Some(SymbolLocator {
            name: owner_name.to_owned(),
            kind: owner_kind,
            span: owner_span.clone(),
        }),
    }
}
