#![cfg(any(feature = "lang-rust", feature = "search"))]

mod tool_fixture;

use blazingly_json::json;
use tool_fixture::Fixture;
use weavatrix_rust::{Weavatrix, tools};

#[test]
#[cfg(all(
    feature = "lang-rust",
    feature = "search",
    feature = "semantic",
    feature = "vector"
))]
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
    assert!(search["occurrences"].as_u64().unwrap() >= 2);
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

    let vector = tools::call(
        &mut engine,
        "vector_search",
        json!({
            "vectors": [
                {"node": "alpha", "values": [1.0, 0.0]},
                {"node": "beta", "values": [0.0, 1.0]}
            ],
            "query": [0.9, 0.1],
            "top_k": 1,
            "exact": true
        }),
    )
    .unwrap();
    assert_eq!(vector["hits"][0]["node"], "alpha");
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
    assert_eq!(search["returned_matches"], 2);
    assert_eq!(search["totals"]["returned_matches"], 2);
    assert_eq!(search["matching_lines"], 5);
    assert_eq!(search["totals"]["matching_lines"], 5);
    assert_eq!(search["occurrences"], 5);
    assert_eq!(search["totals"]["occurrences"], 5);
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
