mod dify_support;

use blazingly_json::json;
use dify_support::{dump, engine, labels};
use weavatrix_rust::tools;

#[test]
fn kubernetes_inventory_is_not_a_dify_app() {
    let (_fixture, mut engine) = engine();
    let inventory = tools::call(&mut engine, "dify_inventory", json!({})).unwrap();
    let apps = inventory["apps"].as_array().unwrap();
    assert!(
        apps.iter()
            .all(|item| item["label"] != "demo-config" && item["label"] != "ConfigMap"),
        "{}",
        dump(&inventory)
    );
    assert!(
        engine
            .state()
            .graph()
            .nodes()
            .iter()
            .any(|node| node.label == "ConfigMap/demo-config"),
        "k8s inventory must still land on the line scanner"
    );
}

#[test]
fn inventory_trace_and_context_keep_marker_consumers() {
    let (_fixture, mut engine) = engine();
    let inventory = tools::call(&mut engine, "dify_inventory", json!({})).unwrap();
    let apps = inventory["apps"].as_array().unwrap();
    assert!(
        apps.iter().any(|item| item["label"] == "llm-simple"),
        "{}",
        dump(&inventory)
    );
    let llm = labels(&engine, "LLM");
    assert_eq!(llm.len(), 1, "{llm:?}");
    let trace = tools::call(
        &mut engine,
        "dify_trace",
        json!({"label": "LLM", "depth": 6, "max_nodes": 100}),
    )
    .unwrap();
    let steps = dump(&trace);
    assert!(
        steps.contains("flows_to") || steps.contains("depends_on_output"),
        "{steps}"
    );
    let context = tools::call(
        &mut engine,
        "dify_context",
        json!({"label": "LLM", "task": "change start_node.query"}),
    )
    .unwrap();
    let text = dump(&context);
    assert!(
        text.contains("start_node.query") || text.contains("marker:start_node.query"),
        "{text}"
    );
    assert!(
        text.contains("runtime response shape is not provided"),
        "{text}"
    );
    assert_eq!(context["coverage"]["runtime"]["provided"], false);
}

#[test]
fn same_display_names_in_different_apps_do_not_merge() {
    let (_fixture, mut engine) = engine();
    let ids = labels(&engine, "Shared Title");
    assert_eq!(ids.len(), 2, "{ids:?}");
    let error =
        tools::call(&mut engine, "dify_trace", json!({"label": "Shared Title"})).unwrap_err();
    assert!(error.contains("ambiguous"), "{error}");
}

#[test]
fn chat_mode_is_recognized_without_a_fake_empty_graph() {
    let (_fixture, engine) = engine();
    let labels = engine
        .state()
        .graph()
        .nodes()
        .iter()
        .map(|node| node.label.as_str())
        .collect::<Vec<_>>();
    assert!(labels.contains(&"Plain Chat"), "{labels:?}");
    assert!(
        labels.contains(&"mode:other") || labels.contains(&"semantics:structure_only"),
        "{labels:?}"
    );
}

#[test]
fn dify_profile_exposes_the_three_domain_tools() {
    let catalog = tools::catalog_for_profile(tools::ToolProfile::Dify);
    let names = catalog.iter().map(|tool| tool.name).collect::<Vec<_>>();
    assert!(names.contains(&"dify_inventory"));
    assert!(names.contains(&"dify_trace"));
    assert!(names.contains(&"dify_context"));
    assert!(!names.contains(&"verified_change"));
    assert!(!names.contains(&"n8n_inventory"));
}
