mod n8n_support;

use blazingly_json::json;
use n8n_support::{dump, engine, labels};
use weavatrix_rust::tools;

#[test]
fn ordinary_json_manifests_do_not_become_n8n() {
    let (_fixture, engine) = engine();
    assert!(
        engine
            .state()
            .graph()
            .nodes()
            .iter()
            .filter(|node| node.label == "n8n-fixtures")
            .all(|node| node.kind.as_str() != "n8n.workflow")
    );
}

#[test]
fn inventory_trace_and_context_work_on_one_export_catalog() {
    let (_fixture, mut engine) = engine();
    let inventory = tools::call(&mut engine, "n8n_inventory", json!({})).unwrap();
    let workflows = inventory["workflows"].as_array().unwrap();
    assert!(
        workflows
            .iter()
            .any(|item| item["label"] == "Invoice Reminder"),
        "{}",
        dump(&inventory)
    );
    assert!(
        workflows
            .iter()
            .any(|item| item["label"] == "Load Customer Workflow"),
        "provided subworkflow must stay in the catalog: {}",
        dump(&inventory)
    );
    let send = labels(&engine, "Send Invoice");
    assert_eq!(send.len(), 1, "{send:?}");
    let trace = tools::call(
        &mut engine,
        "n8n_trace",
        json!({"label": "Send Invoice", "depth": 6, "max_nodes": 100}),
    )
    .unwrap();
    let steps = dump(&trace);
    assert!(
        steps.contains("flows_to") || steps.contains("depends_on_output"),
        "{steps}"
    );
    assert!(!steps.to_ascii_lowercase().contains("infinite"));
    let context = tools::call(
        &mut engine,
        "n8n_context",
        json!({"label": "Send Invoice", "task": "change email recipient"}),
    )
    .unwrap();
    let text = dump(&context);
    assert!(
        text.contains("email") || text.contains("Load Customer"),
        "{text}"
    );
    assert!(
        text.contains("runtime response shape is not provided"),
        "{text}"
    );
    assert_eq!(context["coverage"]["runtime"]["provided"], false);
}

#[test]
fn if_and_merge_keep_every_port_index() {
    let (_fixture, engine) = engine();
    let labels = engine
        .state()
        .graph()
        .nodes()
        .iter()
        .map(|node| node.label.as_str())
        .collect::<Vec<_>>();
    let joined = labels.join(" | ");
    assert!(
        joined.contains("Check Customer/main:0 -> Branch A/main:0"),
        "{joined}"
    );
    assert!(
        joined.contains("Check Customer/main:1 -> Branch B/main:0"),
        "{joined}"
    );
    assert!(
        joined.contains("Branch B/main:0 -> Merge/main:1"),
        "merge input 1 must survive: {joined}"
    );
}

#[test]
fn same_display_names_in_different_workflows_do_not_merge() {
    let (_fixture, mut engine) = engine();
    let ids = labels(&engine, "Load Customer");
    assert_eq!(ids.len(), 2, "{ids:?}");
    let error =
        tools::call(&mut engine, "n8n_trace", json!({"label": "Load Customer"})).unwrap_err();
    assert!(error.contains("ambiguous"), "{error}");
}

#[test]
fn n8n_profile_exposes_the_three_domain_tools() {
    let catalog = tools::catalog_for_profile(tools::ToolProfile::N8n);
    let names = catalog.iter().map(|tool| tool.name).collect::<Vec<_>>();
    assert!(names.contains(&"n8n_inventory"));
    assert!(names.contains(&"n8n_trace"));
    assert!(names.contains(&"n8n_context"));
    assert!(!names.contains(&"verified_change"));
}
