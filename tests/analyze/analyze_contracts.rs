use weavatrix_rust::{Analyzer, CapabilityState, EdgeKind, NodeKind, SourceInput};

fn analyze(sources: &[(&str, &str)]) -> weavatrix_rust::Snapshot {
    Analyzer::default()
        .analyze_sources(
            std::env::current_dir().unwrap(),
            "contract-test",
            sources.iter().map(|(path, source)| SourceInput {
                path: (*path).to_owned(),
                bytes: source.as_bytes().to_vec(),
                content_hash: None,
            }),
        )
        .unwrap()
}

#[test]
fn connects_graphql_schema_fields_to_cross_file_operation_calls() {
    let snapshot = analyze(&[
        (
            "api/schema.graphql",
            "schema { query: Root }\ntype Root { user(id: ID!): User }\ntype User { id: ID! }\n",
        ),
        (
            "web/get-user.gql",
            "query GetUser($id: ID!) { alias: user(id: $id) { id } }\n",
        ),
    ]);
    let endpoint = snapshot
        .nodes
        .iter()
        .find(|node| node.kind == NodeKind::Endpoint && node.label == "GRAPHQL QUERY user")
        .unwrap();
    let schema_edge = snapshot.edges.iter().find(|edge| {
        edge.kind == EdgeKind::Exposes
            && edge.target == endpoint.id
            && edge.source.as_str().contains("struct:Root")
    });
    let operation_edge = snapshot.edges.iter().find(|edge| {
        edge.kind == EdgeKind::Calls
            && edge.target == endpoint.id
            && edge.source.as_str().contains("function:GetUser")
    });
    for edge in [schema_edge, operation_edge] {
        let span = edge
            .expect("schema and operation must meet at one endpoint")
            .provenance
            .span
            .as_ref()
            .expect("contract edge must carry parsed source evidence");
        assert!(span.start.line > 0);
        assert!(span.start.column > 0);
    }
    assert!(snapshot.nodes.iter().any(|node| {
        node.kind == NodeKind::Custom("graphql_field".to_owned())
            && node.label == "GRAPHQL FIELD User.id"
    }));
    assert!(snapshot.capabilities.iter().any(|capability| {
        capability.id == "lang:graphql" && capability.state == CapabilityState::Complete
    }));
}

#[test]
fn connects_proto3_service_rpc_messages_and_streaming_contract() {
    let snapshot = analyze(&[
        (
            "proto/messages.proto",
            "syntax = \"proto3\";\npackage shop.v1;\nmessage WatchRequest {}\nmessage Item {}\n",
        ),
        (
            "proto/inventory.proto",
            "syntax = \"proto3\";\npackage shop.v1;\nimport \"messages.proto\";\nservice Inventory {\n  rpc Watch(stream WatchRequest) returns (stream Item);\n}\n",
        ),
    ]);
    let service = snapshot
        .nodes
        .iter()
        .find(|node| node.kind == NodeKind::Service && node.label == "Inventory")
        .unwrap();
    let method = snapshot
        .nodes
        .iter()
        .find(|node| node.kind == NodeKind::Method && node.label == "Watch")
        .unwrap();
    assert!(snapshot.edges.iter().any(|edge| {
        edge.kind == EdgeKind::Method && edge.source == service.id && edge.target == method.id
    }));
    assert!(snapshot.edges.iter().any(|edge| {
        edge.kind == EdgeKind::Imports
            && edge.source.as_str() == "file:proto/inventory.proto"
            && edge.target.as_str() == "file:proto/messages.proto"
    }));
    for message in ["WatchRequest", "Item"] {
        let target = snapshot
            .nodes
            .iter()
            .find(|node| node.kind == NodeKind::Struct && node.label == message)
            .unwrap();
        assert!(snapshot.edges.iter().any(|edge| {
            edge.kind == EdgeKind::References
                && edge.source == method.id
                && edge.target == target.id
                && edge.provenance.span.is_some()
        }));
    }
    assert!(snapshot.nodes.iter().any(|node| {
        node.kind == NodeKind::Endpoint
            && node.label == "GRPC shop.v1.Inventory/Watch [bidi-streaming]"
    }));
    assert_eq!(
        snapshot
            .nodes
            .iter()
            .filter(|node| node.kind == NodeKind::Custom("grpc_stream".to_owned()))
            .count(),
        2
    );
    assert!(snapshot.capabilities.iter().any(|capability| {
        capability.id == "lang:protobuf" && capability.state == CapabilityState::Complete
    }));
}

#[test]
fn connects_proto2_and_supported_editions_service_contracts() {
    for (path, source, dialect, endpoint) in [
        (
            "proto/legacy.proto",
            concat!(
                "syntax = \"proto2\";\n",
                "package legacy.v1;\n",
                "message Request { optional string id = 1; }\n",
                "message Reply {}\n",
                "service Legacy { rpc Get(Request) returns (Reply); }\n",
            ),
            "proto2",
            "GRPC legacy.v1.Legacy/Get [unary]",
        ),
        (
            "proto/modern.proto",
            concat!(
                "edition = \"2023\";\n",
                "package modern.v1;\n",
                "message Request { string id = 1; }\n",
                "message Reply {}\n",
                "service Modern { rpc Get(Request) returns (Reply); }\n",
            ),
            "edition-2023",
            "GRPC modern.v1.Modern/Get [unary]",
        ),
        (
            "proto/modern-2024.proto",
            concat!(
                "edition = \"2024\";\n",
                "import option \"custom_options.proto\";\n",
                "package modern.v2;\n",
                "message Request { string id = 1; }\n",
                "message Reply {}\n",
                "service Modern { rpc Get(Request) returns (Reply); }\n",
            ),
            "edition-2024",
            "GRPC modern.v2.Modern/Get [unary]",
        ),
    ] {
        let snapshot = analyze(&[
            (path, source),
            ("proto/custom_options.proto", "edition = \"2024\";\n"),
        ]);
        assert!(
            snapshot.diagnostics.is_empty(),
            "{:?}",
            snapshot.diagnostics
        );
        assert!(snapshot.nodes.iter().any(|node| {
            node.kind == NodeKind::Custom("protobuf_dialect".to_owned())
                && node.label == format!("PROTOBUF DIALECT {dialect}")
        }));
        assert!(
            snapshot
                .nodes
                .iter()
                .any(|node| { node.kind == NodeKind::Endpoint && node.label == endpoint })
        );
        if source.contains("import option") {
            assert!(snapshot.edges.iter().any(|edge| {
                edge.kind == EdgeKind::Imports
                    && edge.source.as_str() == "file:proto/modern-2024.proto"
                    && edge.target.as_str() == "file:proto/custom_options.proto"
                    && edge.provenance.span.is_some()
            }));
        }
    }
}

#[test]
fn malformed_graphql_and_invalid_protobuf_dialects_fail_closed() {
    let snapshot = analyze(&[
        ("bad.graphql", "type Query { broken: String\n"),
        (
            "legacy.proto",
            "syntax = \"proto1\";\nmessage Legacy { optional string id = 1; }\n",
        ),
        (
            "future.proto",
            "edition = \"2026\";\nmessage Future { string id = 1; }\n",
        ),
    ]);
    assert!(
        !snapshot
            .nodes
            .iter()
            .any(|node| matches!(node.kind, NodeKind::Endpoint | NodeKind::Struct)),
        "invalid or malformed contracts must not invent graph nodes"
    );
    let graphql = snapshot
        .diagnostics
        .iter()
        .find(|diagnostic| diagnostic.code == "graphql.syntax_error")
        .and_then(|diagnostic| diagnostic.span.as_ref())
        .expect("malformed GraphQL must carry an exact span");
    assert_eq!((graphql.start.line, graphql.start.column), (1, 12));
    for file in ["legacy.proto", "future.proto"] {
        let protobuf = snapshot
            .diagnostics
            .iter()
            .filter(|diagnostic| diagnostic.code == "protobuf.invalid_dialect")
            .filter_map(|diagnostic| diagnostic.span.as_ref())
            .find(|span| span.file == file)
            .expect("invalid protobuf dialect must carry an exact span");
        assert_eq!((protobuf.start.line, protobuf.start.column), (1, 1));
    }
}
