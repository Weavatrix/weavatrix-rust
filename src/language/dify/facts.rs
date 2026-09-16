use super::model::{AppRecord, DomainBatch, DomainRecord, LinkRecord, app_kind, node_kind};
use crate::language::{
    BoundEdgeFact, DomainFact, FileFacts, ReferenceFact, SymbolFact, SymbolLocator,
};
use std::collections::BTreeMap;
use weavatrix_graph::{EdgeKind, NodeKind, SourceSpan};

pub(super) fn to_file_facts(path: &str, batch: &DomainBatch) -> FileFacts {
    let mut facts = FileFacts {
        diagnostics: batch.diagnostics.clone(),
        ..FileFacts::default()
    };
    let mut locators = BTreeMap::<String, SymbolLocator>::new();
    for app in &batch.apps {
        let app_symbol = symbol(&app.name, app_kind(), app.span.clone());
        locators.insert(app.key.clone(), locator(&app_symbol));
        facts.symbols.push(app_symbol);
        facts.domains.push(DomainFact {
            name: format!("mode:{}", app.mode),
            kind: NodeKind::Unknown,
            relation: EdgeKind::Configures,
            span: app.span.clone(),
            owner: Some(SymbolLocator {
                name: app.name.clone(),
                kind: app_kind(),
                span: app.span.clone(),
            }),
        });
        facts.domains.push(DomainFact {
            name: format!("dsl:{}", app.dsl_version),
            kind: NodeKind::Unknown,
            relation: EdgeKind::Configures,
            span: app.span.clone(),
            owner: Some(SymbolLocator {
                name: app.name.clone(),
                kind: app_kind(),
                span: app.span.clone(),
            }),
        });
        for variable in &app.variables {
            let label = format!("{}:{}", variable.scope, variable.name);
            let variable_symbol = symbol(&label, NodeKind::ConfigKey, variable.span.clone());
            locators.insert(variable.key.clone(), locator(&variable_symbol));
            facts.references.push(reference(
                &app.name,
                app_kind(),
                &app.span,
                &label,
                EdgeKind::Contains,
                variable.span.clone(),
            ));
            facts.symbols.push(variable_symbol);
        }
        for node in &app.nodes {
            let node_symbol = symbol(&display_name(node), node_kind(), node.span.clone());
            locators.insert(node.key.clone(), locator(&node_symbol));
            facts.references.push(reference(
                &app.name,
                app_kind(),
                &app.span,
                &display_name(node),
                EdgeKind::Contains,
                node.span.clone(),
            ));
            facts.symbols.push(node_symbol);
            facts.domains.push(DomainFact {
                name: format!("type:{}", node.type_name),
                kind: NodeKind::TypeAlias,
                relation: EdgeKind::Configures,
                span: node.span.clone(),
                owner: Some(SymbolLocator {
                    name: display_name(node),
                    kind: node_kind(),
                    span: node.span.clone(),
                }),
            });
            facts.domains.push(DomainFact {
                name: format!("node_id:{}", node.id),
                kind: NodeKind::Unknown,
                relation: EdgeKind::Configures,
                span: node.span.clone(),
                owner: Some(SymbolLocator {
                    name: display_name(node),
                    kind: node_kind(),
                    span: node.span.clone(),
                }),
            });
        }
        for link in &app.links {
            emit_link(&mut facts, &locators, link);
        }
        for domain in &app.domains {
            emit_domain(&mut facts, &locators, domain);
        }
        emit_coverage(&mut facts, app);
    }
    if batch.truncated {
        facts.diagnostics.push(crate::model::Diagnostic {
            code: "dify.truncated".into(),
            message: "Dify analysis stopped at a declared limit".into(),
            span: Some(super::super::yaml_doc::span_for(path, "", 0, 1)),
        });
    }
    facts
}

fn display_name(node: &super::model::NodeRecord) -> String {
    if node.title.is_empty() {
        node.id.clone()
    } else {
        node.title.clone()
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

fn emit_coverage(facts: &mut FileFacts, app: &AppRecord) {
    for name in [
        format!(
            "coverage:structure:{}/{}",
            app.coverage.structure_accepted, app.coverage.structure_seen
        ),
        format!(
            "coverage:nodeSemantics:{}/{}",
            app.coverage.semantics_supported, app.coverage.semantics_seen
        ),
        format!(
            "coverage:variables:{}/{}",
            app.coverage.variables_resolved, app.coverage.variables_seen
        ),
        "coverage:runtime:false".into(),
    ] {
        facts.domains.push(DomainFact {
            name,
            kind: NodeKind::Unknown,
            relation: EdgeKind::Configures,
            span: app.span.clone(),
            owner: Some(SymbolLocator {
                name: app.name.clone(),
                kind: app_kind(),
                span: app.span.clone(),
            }),
        });
    }
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
