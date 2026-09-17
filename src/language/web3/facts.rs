use super::abi::{AbiDocument, Completeness};
use super::clients::ConsumerOccurrence;
use super::identity::member_key;
use super::kinds::{abi_kind, artifact_kind, binds, consumer_kind, member_kind};
use super::span::file_span;
use crate::language::{BoundEdgeFact, DomainFact, FileFacts, SymbolFact, SymbolLocator};
use crate::model::Diagnostic;
use weavatrix_graph::{EdgeKind, NodeKind};

pub(super) fn documents_to_facts(
    path: &str,
    documents: &[AbiDocument],
    extra: Vec<Diagnostic>,
) -> FileFacts {
    let mut facts = FileFacts {
        diagnostics: extra,
        ..FileFacts::default()
    };
    for document in documents {
        emit_document(&mut facts, path, document);
    }
    facts
}

fn emit_document(facts: &mut FileFacts, path: &str, document: &AbiDocument) {
    let label = document
        .contract_name
        .clone()
        .unwrap_or_else(|| file_label(path));
    let container = symbol(&label, artifact_kind(), document_span(path, document));
    let owner = locator(&container);
    facts.symbols.push(container);
    facts.domains.push(domain(
        format!("web3.profile:{}", document.profile.as_str()),
        NodeKind::Unknown,
        owner.clone(),
    ));
    facts.domains.push(domain(
        format!("web3.provenance:{}", document.provenance),
        NodeKind::Unknown,
        owner.clone(),
    ));
    facts.domains.push(domain(
        format!("web3.completeness:{}", document.completeness.as_str()),
        NodeKind::Unknown,
        owner.clone(),
    ));
    facts.domains.push(domain(
        "web3.deployment:not_provided".into(),
        NodeKind::Unknown,
        owner.clone(),
    ));
    if document.profile == super::abi::Profile::Hardhat3Incomplete {
        facts.diagnostics.push(Diagnostic {
            code: "web3.artifact.incomplete".into(),
            message: "Hardhat 3 artifact without a paired output is incomplete; compiler facts were not fabricated".into(),
            span: Some(file_span(path)),
        });
    }
    for member in &document.members {
        emit_member(facts, path, &owner, member);
    }
    let _ = Completeness::Full;
}

fn emit_member(
    facts: &mut FileFacts,
    path: &str,
    owner: &SymbolLocator,
    member: &super::abi::AbiMember,
) {
    let kind = member_kind(member.kind.as_str());
    let item = symbol(&member.call_signature, kind.clone(), member.span.clone());
    let loc = locator(&item);
    facts.bound_edges.push(BoundEdgeFact {
        from: owner.clone(),
        to: loc.clone(),
        kind: EdgeKind::Contains,
        span: member.span.clone(),
        detail: format!("abi-member:{}", member.kind.as_str()),
    });
    facts.symbols.push(item);
    facts.domains.push(domain(
        format!(
            "web3.member_key:{}",
            member_key(path, member.kind.as_str(), &member.call_signature)
        ),
        abi_kind(),
        loc.clone(),
    ));
    facts.domains.push(domain(
        format!("web3.call:{}", member.call_signature),
        kind.clone(),
        loc.clone(),
    ));
    if !member.return_shape.is_empty() {
        facts.domains.push(domain(
            format!("web3.return:{}", member.return_shape),
            NodeKind::TypeAlias,
            loc.clone(),
        ));
    }
    if !member.event_layout.is_empty() {
        facts.domains.push(domain(
            format!("web3.event_layout:{}", member.event_layout),
            kind,
            loc.clone(),
        ));
    }
    facts.domains.push(domain(
        format!("web3.client_surface:{}", member.client_surface),
        NodeKind::Unknown,
        loc.clone(),
    ));
    if let Some(selector) = &member.selector {
        facts.domains.push(domain(
            format!("web3.selector:{selector}"),
            NodeKind::Unknown,
            loc.clone(),
        ));
    }
    if member.unsupported {
        facts.domains.push(domain(
            "web3.support:unsupported".into(),
            NodeKind::Unknown,
            loc,
        ));
    }
}

pub(super) fn emit_consumers(
    facts: &mut FileFacts,
    path: &str,
    occurrences: &[ConsumerOccurrence],
) {
    for occurrence in occurrences {
        let label = occurrence
            .member_name
            .clone()
            .unwrap_or_else(|| occurrence.api.clone());
        let item = symbol(
            &format!("{}#{}", occurrence.api, occurrence.ordinal),
            consumer_kind(),
            occurrence.span.clone(),
        );
        let loc = locator(&item);
        facts.symbols.push(item);
        facts.domains.push(domain(
            format!("web3.consumer_api:{}", occurrence.api),
            NodeKind::Unknown,
            loc.clone(),
        ));
        facts.domains.push(domain(
            format!(
                "web3.binding:{}",
                if occurrence.resolved {
                    "matched_artifact"
                } else {
                    "unresolved"
                }
            ),
            NodeKind::Unknown,
            loc.clone(),
        ));
        if let Some(name) = &occurrence.member_name {
            facts.domains.push(domain(
                format!("web3.needs:{}:{name}", occurrence.member_kind),
                consumer_kind(),
                loc.clone(),
            ));
        }
        if let Some(local) = &occurrence.abi_local {
            facts.domains.push(domain(
                format!("web3.abi_local:{local}"),
                NodeKind::Unknown,
                loc.clone(),
            ));
        }
        if let Some(import_path) = &occurrence.import_path {
            let key = match &occurrence.member_name {
                Some(name) if occurrence.resolved => {
                    format!("{import_path}#{}:{name}", occurrence.member_kind)
                }
                _ => format!("{import_path}#abi"),
            };
            facts.domains.push(domain(
                format!("web3.bind:{key}"),
                consumer_kind(),
                loc.clone(),
            ));
        }
        if !occurrence.resolved {
            facts.domains.push(domain(
                "web3.unresolved:dynamic_or_spread".into(),
                NodeKind::Unknown,
                loc.clone(),
            ));
        }
        facts.domains.push(domain(
            format!("web3.occurrence:{}:{}", path, occurrence.ordinal),
            NodeKind::Unknown,
            loc,
        ));
        let _ = binds();
        let _ = label;
    }
}

fn symbol(name: &str, kind: NodeKind, span: weavatrix_graph::SourceSpan) -> SymbolFact {
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

fn domain(name: String, kind: NodeKind, owner: SymbolLocator) -> DomainFact {
    DomainFact {
        name,
        kind,
        relation: EdgeKind::Configures,
        span: owner.span.clone(),
        owner: Some(owner),
    }
}

fn file_label(path: &str) -> String {
    std::path::Path::new(path)
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or(path)
        .to_owned()
}

fn document_span(path: &str, document: &AbiDocument) -> weavatrix_graph::SourceSpan {
    document
        .members
        .first()
        .map_or_else(|| file_span(path), |member| member.span.clone())
}
