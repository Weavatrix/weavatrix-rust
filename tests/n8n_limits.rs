mod n8n_support;

use blazingly_json::json;
use n8n_support::{Fixture, dump, engine, labels};
use weavatrix_rust::{Analyzer, SourceInput, tools};

#[test]
fn cyclic_workflow_is_not_an_infinite_execution_error() {
    let (_fixture, mut engine) = engine();
    let trace = tools::call(
        &mut engine,
        "n8n_trace",
        json!({"label": "Loop A", "depth": 8}),
    )
    .unwrap();
    let text = dump(&trace);
    assert!(!text.to_ascii_lowercase().contains("infinite"));
    assert_eq!(trace["cycle"], true);
    assert!(
        engine
            .state()
            .snapshot()
            .diagnostics
            .iter()
            .all(|item| !item.message.to_ascii_lowercase().contains("infinite"))
    );
}

#[test]
fn unknown_community_node_keeps_structure_and_marks_limits() {
    let (_fixture, mut engine) = engine();
    assert_eq!(labels(&engine, "Vendor Sync").len(), 1);
    let context = tools::call(&mut engine, "n8n_context", json!({"label": "Vendor Sync"})).unwrap();
    let text = dump(&context);
    assert!(
        text.contains("structure_only") || text.contains("unsupported") || text.contains("runtime"),
        "{text}"
    );
}

#[test]
fn missing_subworkflow_is_not_declared_deleted() {
    let (_fixture, mut engine) = engine();
    let inventory = tools::call(&mut engine, "n8n_inventory", json!({})).unwrap();
    let text = dump(&inventory);
    assert!(
        text.contains("wf-not-provided") || text.contains("subworkflow:database:wf-not-provided")
    );
    assert!(!text.to_ascii_lowercase().contains("deleted"));
    assert!(
        engine
            .state()
            .snapshot()
            .diagnostics
            .iter()
            .all(|item| !item.message.to_ascii_lowercase().contains("deleted"))
    );
    assert!(
        engine
            .state()
            .graph()
            .nodes()
            .iter()
            .any(|node| node.label == "subworkflow:no-wait")
    );
}

#[test]
fn historical_blobs_are_not_reread_from_the_worktree() {
    let fixture = Fixture::empty();
    let old = include_str!("fixtures/n8n/load-customer.json");
    fixture.write("workflows/load-customer.json", old);
    let snapshot = Analyzer::default()
        .analyze_sources(
            &fixture.root,
            "old",
            [SourceInput {
                path: "workflows/load-customer.json".into(),
                bytes: old.as_bytes().to_vec(),
                content_hash: None,
            }],
        )
        .unwrap();
    fixture.write(
        "workflows/load-customer.json",
        r#"{"id":"wf-load-customer","name":"NEW WORKTREE","nodes":[{"id":"x","name":"New","type":"n8n-nodes-base.noOp","typeVersion":1,"parameters":{}}],"connections":{}}"#,
    );
    assert!(
        snapshot
            .nodes
            .iter()
            .any(|node| node.label == "Fetch Record")
    );
    assert!(
        snapshot
            .nodes
            .iter()
            .all(|node| node.label != "NEW WORKTREE")
    );
    assert!(snapshot.nodes.iter().all(|node| node.label != "New"));
}

#[test]
fn secret_http_values_do_not_reach_default_context() {
    let (_fixture, mut engine) = engine();
    let context = tools::call(&mut engine, "n8n_context", json!({"label": "Paid API"})).unwrap();
    let text = dump(&context);
    assert!(!text.contains("super-secret"), "{text}");
    assert!(!text.contains("leaked-token"), "{text}");
    assert!(!text.contains("user:super-secret@"), "{text}");
    let inventory = tools::call(&mut engine, "n8n_inventory", json!({})).unwrap();
    let inventory_text = dump(&inventory);
    assert!(!inventory_text.contains("super-secret"));
    assert!(!inventory_text.contains("leaked-token"));
}

#[test]
fn clipboard_export_is_partial_not_full() {
    let (_fixture, engine) = engine();
    assert!(
        engine
            .state()
            .graph()
            .nodes()
            .iter()
            .any(|node| node.label == "completeness:partial")
    );
    assert!(
        engine
            .state()
            .snapshot()
            .diagnostics
            .iter()
            .any(|item| item.code == "n8n.partial")
    );
}

#[test]
fn invalid_n8n_shape_stays_visible_with_diagnostics() {
    let snapshot = Analyzer::default()
        .analyze_sources(
            std::env::current_dir().unwrap(),
            "invalid",
            [SourceInput {
                path: "workflows/broken.json".into(),
                bytes: br#"{"nodes":[{"id":1}],"connections":{"left":"right"}}"#.to_vec(),
                content_hash: None,
            }],
        )
        .unwrap();
    assert!(
        snapshot
            .diagnostics
            .iter()
            .any(|item| item.code == "n8n.invalid_workflow")
    );
}
