//! Protobuf/gRPC graph adapter over `weavatrix-parse`'s lossless typed facts.

use super::contract::{add_symbol, facts_with_diagnostics, source_span};
use super::{
    DomainFact, FileFacts, ImportFact, Language, LanguageAdapter, ReferenceFact, SourceFile,
};
use crate::Result;
use std::collections::BTreeMap;
use weavatrix_graph::{EdgeKind, NodeKind};
use weavatrix_parse::{ContractKind, TokenKind};

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

    // Keep the service/RPC/request/response mapping together so streaming
    // flags cannot diverge between the endpoint and its configuration facts.
    #[allow(clippy::too_many_lines)]
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
            let contract_span = source_span(source.path, &contract.span);
            match contract.kind {
                ContractKind::ProtobufPackage => {
                    package.clone_from(&contract.name);
                    add_symbol(
                        &mut facts,
                        &mut owners,
                        contract.name,
                        NodeKind::Package,
                        contract_span,
                        None,
                    );
                }
                ContractKind::ProtobufMessage => {
                    add_symbol(
                        &mut facts,
                        &mut owners,
                        contract.name,
                        NodeKind::Struct,
                        contract_span,
                        None,
                    );
                }
                ContractKind::ProtobufEnum => {
                    add_symbol(
                        &mut facts,
                        &mut owners,
                        contract.name,
                        NodeKind::Enum,
                        contract_span,
                        None,
                    );
                }
                ContractKind::ProtobufService => {
                    add_symbol(
                        &mut facts,
                        &mut owners,
                        contract.name,
                        NodeKind::Service,
                        contract_span,
                        None,
                    );
                }
                ContractKind::ProtobufRpc {
                    input,
                    output,
                    client_streaming,
                    server_streaming,
                } => {
                    let service = contract.owner.unwrap_or_default();
                    let method = add_symbol(
                        &mut facts,
                        &mut owners,
                        contract.name.clone(),
                        NodeKind::Method,
                        contract_span.clone(),
                        Some(service.clone()),
                    );
                    for message in [input, output] {
                        facts.references.push(ReferenceFact {
                            name: local_type(&message, &package),
                            kind: EdgeKind::References,
                            receiver: None,
                            qualified: false,
                            span: contract_span.clone(),
                            owner: Some(method.clone()),
                        });
                    }
                    let qualified = if package.is_empty() {
                        format!("{service}/{}", contract.name)
                    } else {
                        format!("{package}.{service}/{}", contract.name)
                    };
                    let mode = match (client_streaming, server_streaming) {
                        (true, true) => "bidi-streaming",
                        (true, false) => "client-streaming",
                        (false, true) => "server-streaming",
                        (false, false) => "unary",
                    };
                    facts.domains.push(DomainFact {
                        name: format!("GRPC {qualified} [{mode}]"),
                        kind: NodeKind::Endpoint,
                        relation: EdgeKind::Exposes,
                        span: contract_span.clone(),
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
                            span: contract_span.clone(),
                            owner: Some(method.clone()),
                        });
                    }
                }
                _ => {}
            }
        }
        Ok(facts)
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
mod tests {
    use super::*;

    #[test]
    fn converts_services_rpcs_messages_and_streaming() {
        let source = concat!(
            "syntax = \"proto3\";\n",
            "package shop.v1;\n",
            "message Request {}\n",
            "message Reply {}\n",
            "service Inventory { rpc Watch(stream Request) returns (stream Reply); }\n",
        );
        let facts = ProtobufAdapter
            .parse(SourceFile {
                path: "inventory.proto",
                text: source,
            })
            .unwrap();
        assert!(facts.diagnostics.is_empty());
        assert!(
            facts
                .symbols
                .iter()
                .any(|fact| { fact.name == "Inventory" && fact.kind == NodeKind::Service })
        );
        assert!(
            facts
                .domains
                .iter()
                .any(|fact| { fact.name == "GRPC shop.v1.Inventory/Watch [bidi-streaming]" })
        );
        assert_eq!(facts.references.len(), 2);
        assert_eq!(
            facts
                .domains
                .iter()
                .filter(|fact| fact.kind == NodeKind::Custom("grpc_stream".to_owned()))
                .count(),
            2
        );
        assert!(facts.domains.iter().any(|fact| {
            fact.name == "PROTOBUF DIALECT proto3"
                && fact.kind == NodeKind::Custom("protobuf_dialect".to_owned())
        }));
    }

    #[test]
    fn converts_proto2_and_supported_editions_rpc_contracts() {
        for (dialect, source, option_import) in [
            (
                "proto2",
                concat!(
                    "syntax = \"proto2\";\n",
                    "message Request { optional string id = 1; }\n",
                    "message Reply {}\n",
                    "service Legacy { rpc Get(Request) returns (Reply); }\n",
                ),
                None,
            ),
            (
                "edition-2023",
                concat!(
                    "edition = \"2023\";\n",
                    "message Request { string id = 1; }\n",
                    "message Reply {}\n",
                    "service Modern { rpc Get(Request) returns (Reply); }\n",
                ),
                None,
            ),
            (
                "edition-2024",
                concat!(
                    "edition = \"2024\";\n",
                    "import option \"custom_options.proto\";\n",
                    "message Request { string id = 1; }\n",
                    "message Reply {}\n",
                    "service Modern { rpc Get(Request) returns (Reply); }\n",
                ),
                Some("custom_options.proto"),
            ),
        ] {
            let facts = ProtobufAdapter
                .parse(SourceFile {
                    path: "contract.proto",
                    text: source,
                })
                .unwrap();
            assert!(facts.diagnostics.is_empty(), "{facts:?}");
            assert!(facts.domains.iter().any(|fact| {
                fact.name == format!("PROTOBUF DIALECT {dialect}")
                    && fact.kind == NodeKind::Custom("protobuf_dialect".to_owned())
            }));
            assert!(facts.domains.iter().any(|fact| {
                fact.name.ends_with("/Get [unary]") && fact.kind == NodeKind::Endpoint
            }));
            if let Some(option_import) = option_import {
                assert!(facts.imports.iter().any(|fact| {
                    fact.target == option_import
                        && fact.span.file == "contract.proto"
                        && fact.span.start.line == 2
                }));
            }
        }
    }
}
