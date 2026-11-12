//! Graphify-facing regressions: ranked seeds, honest bounds, path witnesses.

mod communities;
mod paths;

use crate::language_fixture::Fixture;
use blazingly_json::json;
use weavatrix_rust::{Weavatrix, tools};

#[test]
fn query_graph_ranks_a_question_without_the_whole_phrase() {
    let fixture = Fixture::new();
    fixture.write(
        "src/auth.js",
        "export function validateSession(token) { return token === 'ok'; }\n",
    );
    fixture.write(
        "src/handler.js",
        "import { validateSession } from './auth.js';\nexport function handle(req) { return validateSession(req.token); }\n",
    );
    fixture.write(
        "pkg/other/auth.js",
        "export function validateSession() { return false; }\n",
    );
    let mut engine = Weavatrix::open(&fixture.root).unwrap();
    let report = tools::call(
        &mut engine,
        "query_graph",
        json!({"question": "who calls validateSession", "depth": 1, "max_nodes": 12}),
    )
    .unwrap();
    let labels = report["resolved_seeds"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|node| node["label"].as_str())
        .collect::<Vec<_>>();
    assert!(
        labels.contains(&"validateSession"),
        "the question must resolve the symbol, not the whole phrase: {report}"
    );
    assert_eq!(report["intent"], "callers");
    assert_eq!(report["execution_status"], "OK");
    let alias = tools::call(
        &mut engine,
        "query_graph",
        json!({"query": "who calls validateSession", "depth": 1, "max_nodes": 12}),
    )
    .unwrap();
    assert_eq!(alias["intent"], "callers");
}

#[test]
fn query_graph_does_not_return_dangling_edge_endpoints() {
    let fixture = Fixture::new();
    fixture.write(
        "src/hub.js",
        "import { left } from './left.js';\nimport { right } from './right.js';\nexport function hub() { return left() + right(); }\n",
    );
    fixture.write("src/left.js", "export function left() { return 1; }\n");
    fixture.write("src/right.js", "export function right() { return 2; }\n");
    let mut engine = Weavatrix::open(&fixture.root).unwrap();
    let report = tools::call(
        &mut engine,
        "query_graph",
        json!({"seed_symbols": ["hub"], "depth": 2, "max_nodes": 2}),
    )
    .unwrap();
    let shown = report["nodes"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|item| item["node"]["id"].as_str())
        .collect::<std::collections::BTreeSet<_>>();
    for edge in report["edges"].as_array().unwrap() {
        let source = edge["source"].as_str().unwrap();
        let target = edge["target"].as_str().unwrap();
        assert!(
            shown.contains(source) && shown.contains(target),
            "every returned edge must have shown endpoints: {edge} shown={shown:?}"
        );
    }
    assert_eq!(report["evidence_completeness"], "INCOMPLETE");
    assert_eq!(report["truncated"], true);
}

#[test]
fn query_graph_edges_keep_kind_and_coupling_without_provenance() {
    let fixture = Fixture::new();
    fixture.write(
        "src/hub.js",
        "import { left } from './left.js';\nexport function hub() { return left(); }\n",
    );
    fixture.write("src/left.js", "export function left() { return 1; }\n");
    let mut engine = Weavatrix::open(&fixture.root).unwrap();
    let report = tools::call(
        &mut engine,
        "query_graph",
        json!({
            "seed_files": ["src/hub.js"],
            "relation_filter": "imports",
            "depth": 1,
            "max_nodes": 12
        }),
    )
    .unwrap();
    let edges = report["edges"].as_array().unwrap();
    assert!(!edges.is_empty(), "{report}");
    for edge in edges {
        assert!(edge.get("provenance").is_none(), "{edge}");
        assert!(edge.get("attributes").is_none(), "{edge}");
        assert!(edge["source"].as_str().is_some(), "{edge}");
        assert!(edge["target"].as_str().is_some(), "{edge}");
        assert!(edge["kind"].as_str().is_some(), "{edge}");
    }
    assert!(
        edges
            .iter()
            .any(|edge| edge["coupling"] == "runtime" && edge["line"].is_number()),
        "{edges:?}"
    );
}
