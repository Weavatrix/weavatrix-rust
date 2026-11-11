#![cfg(any(feature = "lang-rust", feature = "search"))]

use crate::tool_fixture::Fixture;
#[cfg(feature = "search")]
use blazingly_json::json;
use weavatrix_rust::Weavatrix;
#[cfg(feature = "search")]
use weavatrix_rust::tools;

#[test]
#[cfg(all(feature = "lang-rust", feature = "search", feature = "semantic"))]
fn graph_search_source_and_semantic_tools_work_together() {
    let fixture = Fixture::new();
    fixture.write(
        "src/lib.rs",
        "pub fn alpha() { beta(); }\npub fn beta() {}\n",
    );
    fixture.write(
        "web/router.js",
        "function list() {}\nrouter.get(\"/items\", list);\n",
    );
    let mut engine = Weavatrix::open(&fixture.root).unwrap();

    let stats = tools::call(&mut engine, "graph_stats", json!({})).unwrap();
    assert!(stats["nodes"].as_u64().unwrap() >= 6);
    let search = tools::call(
        &mut engine,
        "search_code",
        json!({"query": "beta", "max_results": 10}),
    )
    .unwrap();
    assert!(search["totals"]["occurrences"].as_u64().unwrap() >= 2);
    let endpoints = tools::call(&mut engine, "list_endpoints", json!({})).unwrap();
    assert_eq!(endpoints["endpoints"][0]["label"], "GET /items");

    let nodes = engine
        .state()
        .graph()
        .nodes()
        .iter()
        .filter(|node| node.label == "alpha" || node.label == "beta")
        .map(|node| node.id.to_string())
        .collect::<Vec<_>>();
    let semantic = tools::call(
        &mut engine,
        "semantic_link",
        json!({
            "min_similarity": 0.5,
            "vectors": [
                {"node": nodes[0], "values": [1.0, 0.0]},
                {"node": nodes[1], "values": [0.9, 0.1]}
            ]
        }),
    )
    .unwrap();
    assert!(
        semantic["edges"]
            .as_array()
            .is_some_and(|edges| !edges.is_empty())
    );

    let closed = tools::call(&mut engine, "vector_search", json!({})).unwrap_err();
    assert!(
        closed.contains("is not an MCP tool"),
        "vector_search stays in weavatrix-search-vector: {closed}"
    );
}

#[test]
#[cfg(all(feature = "lang-rust", feature = "search", feature = "semantic"))]
fn phrase_rank_reads_the_doc_comment_and_demotes_the_test_copy() {
    let fixture = Fixture::new();
    fixture.write(
        "src/identity.rs",
        "/// Enforces strict manufacturer identity before an order is accepted.\npub fn guard() {}\n\npub fn manufacturer() {}\n",
    );
    fixture.write(
        "tests/identity.rs",
        "pub fn guard() {}\n\npub fn manufacturer() {}\n",
    );
    let mut engine = Weavatrix::open(&fixture.root).unwrap();

    let phrase = tools::call(
        &mut engine,
        "semantic_link",
        json!({"query": "strict manufacturer identity"}),
    )
    .unwrap();
    assert_eq!(phrase["model"], "weavatrix-token-hash-v1");
    assert_eq!(phrase["neural_embedding"], false);
    let hit = &phrase["hits"][0];
    assert_eq!(hit["path"], "src/identity.rs", "{phrase}");
    assert_eq!(hit["label"], "guard", "{phrase}");
    let matched = hit["matched_tokens"].as_array().unwrap();
    assert!(matched.iter().any(|token| token == "strict"), "{phrase}");
    assert!(
        matched.iter().any(|token| token == "manufacturer"),
        "{phrase}"
    );

    let named = tools::call(
        &mut engine,
        "semantic_link",
        json!({"query": "manufacturer"}),
    )
    .unwrap();
    assert_eq!(named["hits"][0]["path"], "src/identity.rs", "{named}");
    assert_eq!(named["hits"][0]["label"], "manufacturer", "{named}");
}

#[test]
#[cfg(feature = "search")]
fn search_code_bounds_returned_matches_without_losing_totals() {
    let fixture = Fixture::new();
    fixture.write("src/many.rs", "needle\nneedle\nneedle\nneedle\nneedle\n");
    let mut engine = Weavatrix::open(&fixture.root).unwrap();

    let search = tools::call(
        &mut engine,
        "search_code",
        json!({"query": "needle", "max_results": 2}),
    )
    .unwrap();

    assert_eq!(search["matches"].as_array().map(Vec::len), Some(2));
    assert_eq!(search["totals"]["returned_matches"], 2);
    assert_eq!(search["totals"]["matching_lines"], 5);
    assert_eq!(search["totals"]["occurrences"], 5);
    assert!(search.get("occurrences").is_none());
    assert_eq!(search["truncated"], true);
}

#[test]
#[cfg(feature = "lang-rust")]
fn incremental_refresh_rebuilds_only_after_source_changes() {
    let fixture = Fixture::new();
    fixture.write("src/lib.rs", "pub fn first() {}\n");
    let mut engine = Weavatrix::open(&fixture.root).unwrap();
    assert!(!engine.refresh_if_stale().unwrap());
    let before = engine.state().graph().node_count();

    fixture.write("src/lib.rs", "pub fn first() {}\npub fn second() {}\n");
    assert!(engine.refresh_if_stale().unwrap());
    assert!(engine.state().graph().node_count() > before);
}
