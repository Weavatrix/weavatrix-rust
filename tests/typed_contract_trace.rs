#![cfg(all(
    feature = "clone",
    feature = "git",
    feature = "lang-rust",
    feature = "memory",
    feature = "search",
    feature = "semantic",
    feature = "vector"
))]

mod support;

use blazingly_json::json;
use support::GitFixture;
use weavatrix_rust::{Weavatrix, tools};

#[test]
fn public_trace_matches_typed_graphql_and_grpc_contracts_with_provenance() {
    let backend = GitFixture::new();
    backend.write(
        "api/schema.graphql",
        "type Query { user: User }\ntype User { id: ID! }\n",
    );
    backend.write(
        "proto/inventory.proto",
        concat!(
            "syntax = \"proto3\";\n",
            "package shop.v1;\n",
            "message Request {}\n",
            "message Reply {}\n",
            "service Inventory { rpc Get(Request) returns (Reply); }\n",
        ),
    );
    backend.commit("backend contracts");

    let client = GitFixture::new();
    client.write(
        "queries/get-user.graphql",
        "query GetUser { user { id } }\n",
    );
    client.write(
        "proto/inventory.proto",
        concat!(
            "syntax = \"proto3\";\n",
            "package shop.v1;\n",
            "message Request {}\n",
            "message Reply {}\n",
            "service Inventory { rpc Get(Request) returns (Reply); }\n",
        ),
    );
    client.commit("client contracts");

    let mut engine = Weavatrix::open(&backend.root).unwrap();
    for (transport, key, extractor) in [
        ("graphql", "graphql", "weavatrix.parse.graphql"),
        ("grpc", "grpc", "weavatrix.parse.protobuf"),
    ] {
        let result = tools::call(
            &mut engine,
            "trace_api_contract",
            json!({
                "backend": backend.root,
                "clients": [client.root],
                "transport": transport
            }),
        )
        .unwrap();
        assert_eq!(result["status"], "COMPLETE", "{transport}: {result}");
        assert_eq!(result["verdict"]["code"], "MATCHED");
        assert_eq!(result[key]["status"], "COMPLETE");
        assert_eq!(result[key]["totals"]["matches"], 1);
        assert_eq!(result[key]["totals"]["mismatches"], 0);
        let contract = &result[key]["contracts"][0];
        assert_eq!(contract["matched"], true);
        assert!(
            contract["backend"]
                .as_array()
                .into_iter()
                .flatten()
                .any(|evidence| {
                    evidence["extractor"] == extractor
                        && evidence["file"].as_str().is_some()
                        && evidence["line"].as_u64().is_some_and(|line| line > 0)
                        && evidence["column"].as_u64().is_some_and(|column| column > 0)
                }),
            "backend evidence must come from typed parser provenance: {contract}"
        );
        assert!(
            contract["clients"]
                .as_array()
                .into_iter()
                .flatten()
                .any(|evidence| evidence["extractor"] == extractor),
            "client evidence must come from typed parser provenance: {contract}"
        );
    }
}

#[test]
fn public_trace_rejects_failed_closed_contract_files() {
    let backend = GitFixture::new();
    backend.write("api/schema.graphql", "type Query { user: String }\n");
    backend.commit("backend");
    let client = GitFixture::new();
    client.write("queries/get.graphql", "query Get { user }\n");
    client.write("queries/broken.graphql", "type Broken { field: String\n");
    client.commit("client with malformed contract");

    let mut engine = Weavatrix::open(&backend.root).unwrap();
    let error = tools::call(
        &mut engine,
        "trace_api_contract",
        json!({
            "backend": backend.root,
            "clients": [client.root],
            "transport": "graphql"
        }),
    )
    .expect_err("malformed GraphQL evidence must fail closed");
    assert!(error.contains("graphql contract parsing failed closed"));
    assert!(error.contains("graphql.syntax_error"));
    assert!(error.contains("queries/broken.graphql"));
}

#[test]
fn public_trace_reports_grpc_streaming_mode_mismatch() {
    let backend = GitFixture::new();
    backend.write(
        "inventory.proto",
        concat!(
            "syntax = \"proto3\";\n",
            "package shop.v1;\n",
            "message Request {}\n",
            "message Reply {}\n",
            "service Inventory { rpc Watch(Request) returns (Reply); }\n",
        ),
    );
    backend.commit("unary server");
    let client = GitFixture::new();
    client.write(
        "inventory.proto",
        concat!(
            "syntax = \"proto3\";\n",
            "package shop.v1;\n",
            "message Request {}\n",
            "message Reply {}\n",
            "service Inventory { rpc Watch(stream Request) returns (stream Reply); }\n",
        ),
    );
    client.commit("streaming client");

    let mut engine = Weavatrix::open(&backend.root).unwrap();
    let result = tools::call(
        &mut engine,
        "trace_api_contract",
        json!({
            "backend": backend.root,
            "clients": [client.root],
            "transport": "grpc"
        }),
    )
    .unwrap();
    assert_eq!(result["status"], "COMPLETE");
    assert_eq!(result["verdict"]["code"], "TYPED_API_CONTRACT_MISMATCH");
    assert_eq!(result["grpc"]["totals"]["mismatches"], 1);
    assert_eq!(
        result["grpc"]["mismatches"][0]["kind"],
        "STREAMING_MODE_MISMATCH"
    );
    assert_eq!(result["grpc"]["totals"]["unmatched_endpoints"], 1);
}
