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
        assert!(
            facts
                .domains
                .iter()
                .any(|fact| fact.name.ends_with("/Get [unary]") && fact.kind == NodeKind::Endpoint)
        );
        if let Some(option_import) = option_import {
            assert!(facts.imports.iter().any(|fact| {
                fact.target == option_import
                    && fact.span.file == "contract.proto"
                    && fact.span.start.line == 2
            }));
        }
    }
}
