//! Protobuf/gRPC graph adapter over `weavatrix-parse`'s lossless typed facts.

use super::contract::{add_symbol, facts_with_diagnostics, source_span};
use super::{
    DomainFact, FileFacts, ImportFact, Language, LanguageAdapter, ReferenceFact, SourceFile,
    SymbolLocator,
};
use crate::model::Result;
use std::collections::BTreeMap;
use weavatrix_graph::{EdgeKind, NodeKind};
use weavatrix_parse::{Contract, ContractKind, TokenKind};

#[derive(Debug, Clone, Copy)]
pub struct ProtobufAdapter;

impl LanguageAdapter for ProtobufAdapter {
    fn language(&self) -> Language {
        Language::Protobuf
    }

    fn extensions(&self) -> &'static [&'static str] {
        &["proto"]
    }

    fn extractor(&self) -> &'static str {
        "weavatrix.parse.protobuf"
    }

    fn parse(&self, source: SourceFile<'_>) -> Result<FileFacts> {
        let parsed = weavatrix_parse::extract(source.text, weavatrix_parse::Language::Protobuf);
        let mut facts = facts_with_diagnostics(source.path, &parsed.diagnostics);
        if !facts.diagnostics.is_empty() {
            return Ok(facts);
        }
        if let Some((dialect, span)) = protobuf_dialect(source.text) {
            facts.domains.push(DomainFact {
                name: format!("PROTOBUF DIALECT {dialect}"),
                kind: NodeKind::Custom("protobuf_dialect".to_owned()),
                relation: EdgeKind::Configures,
                span: source_span(source.path, &span),
                owner: None,
            });
        }
        for import in parsed.imports {
            let fact = ImportFact::new(import.specifier, source_span(source.path, &import.span));
            if import.reexport {
                facts.reexports.push(fact);
            } else {
                facts.imports.push(fact);
            }
        }

        let mut package = String::new();
        let mut owners = BTreeMap::new();
        for contract in parsed.contracts {
            apply_contract(source.path, &mut facts, &mut owners, &mut package, contract);
        }
        Ok(facts)
    }
}

fn apply_contract(
    path: &str,
    facts: &mut FileFacts,
    owners: &mut BTreeMap<String, SymbolLocator>,
    package: &mut String,
    contract: Contract,
) {
    let Contract {
        name,
        kind,
        span: contract_span,
        owner,
    } = contract;
    let span = source_span(path, &contract_span);
    let symbol_kind = match kind {
        ContractKind::ProtobufPackage => {
            package.clone_from(&name);
            NodeKind::Package
        }
        ContractKind::ProtobufMessage => NodeKind::Struct,
        ContractKind::ProtobufEnum => NodeKind::Enum,
        ContractKind::ProtobufService => NodeKind::Service,
        ContractKind::ProtobufRpc {
            input,
            output,
            client_streaming,
            server_streaming,
        } => {
            add_rpc(
                facts,
                owners,
                package,
                &name,
                owner,
                &span,
                input,
                output,
                client_streaming,
                server_streaming,
            );
            return;
        }
        _ => return,
    };
    add_symbol(facts, owners, name, symbol_kind, span, None);
}

#[allow(clippy::too_many_arguments)]
fn add_rpc(
    facts: &mut FileFacts,
    owners: &mut BTreeMap<String, SymbolLocator>,
    package: &str,
    name: &str,
    owner: Option<String>,
    span: &weavatrix_graph::SourceSpan,
    input: String,
    output: String,
    client_streaming: bool,
    server_streaming: bool,
) {
    let service = owner.unwrap_or_default();
    let method = add_symbol(
        facts,
        owners,
        name.to_string(),
        NodeKind::Method,
        span.clone(),
        Some(service.clone()),
    );
    for message in [input, output] {
        facts.references.push(ReferenceFact {
            name: local_type(&message, package),
            kind: EdgeKind::References,
            receiver: None,
            qualified: false,
            span: span.clone(),
            owner: Some(method.clone()),
        });
    }
    let qualified = qualified_rpc(package, &service, name);
    let mode = streaming_mode(client_streaming, server_streaming);
    facts.domains.push(DomainFact {
        name: format!("GRPC {qualified} [{mode}]"),
        kind: NodeKind::Endpoint,
        relation: EdgeKind::Exposes,
        span: span.clone(),
        owner: Some(method.clone()),
    });
    for direction in [
        client_streaming.then_some("client"),
        server_streaming.then_some("server"),
    ]
    .into_iter()
    .flatten()
    {
        facts.domains.push(DomainFact {
            name: format!("GRPC STREAM {direction} {qualified}"),
            kind: NodeKind::Custom("grpc_stream".to_owned()),
            relation: EdgeKind::Configures,
            span: span.clone(),
            owner: Some(method.clone()),
        });
    }
}

fn qualified_rpc(package: &str, service: &str, method: &str) -> String {
    if package.is_empty() {
        format!("{service}/{method}")
    } else {
        format!("{package}.{service}/{method}")
    }
}

const fn streaming_mode(client: bool, server: bool) -> &'static str {
    match (client, server) {
        (true, true) => "bidi-streaming",
        (true, false) => "client-streaming",
        (false, true) => "server-streaming",
        (false, false) => "unary",
    }
}

fn protobuf_dialect(source: &str) -> Option<(String, weavatrix_parse::Span)> {
    let tokens = weavatrix_parse::tokenize_lite(source, weavatrix_parse::Language::Protobuf);
    for window in tokens.windows(4) {
        if window[1].text(source) != "="
            || window[2].kind != TokenKind::String
            || window[3].text(source) != ";"
        {
            continue;
        }
        let value = window[2].text(source).trim_matches(['"', '\'']);
        let dialect = match (window[0].text(source), value) {
            ("syntax", "proto2" | "proto3") => value.to_owned(),
            ("edition", "2023" | "2024") => format!("edition-{value}"),
            _ => continue,
        };
        let end_column = window[2].column.saturating_add(
            u32::try_from(window[2].text(source).chars().count()).unwrap_or(u32::MAX),
        );
        return Some((
            dialect,
            weavatrix_parse::Span {
                start: window[0].start,
                end: window[2].end,
                line: window[0].line,
                column: window[0].column,
                end_line: window[2].line,
                end_column,
            },
        ));
    }
    None
}

fn local_type(name: &str, package: &str) -> String {
    let name = name.trim_start_matches('.');
    name.strip_prefix(package)
        .and_then(|rest| rest.strip_prefix('.'))
        .unwrap_or(name)
        .to_owned()
}

#[cfg(test)]
mod tests;
