mod agent_support;

use agent_support::{dump, engine};
use blazingly_json::json;
use weavatrix_rust::tools;

#[test]
fn ordinary_package_manifests_do_not_become_plugins() {
    let (_fixture, engine) = engine();
    assert!(
        engine
            .state()
            .graph()
            .nodes()
            .iter()
            .filter(|node| node.label == "agent-fixtures")
            .all(|node| node.kind.as_str() != "agent.plugin")
    );
}

#[test]
fn same_server_name_in_two_packages_stays_distinct() {
    let (_fixture, mut engine) = engine();
    let inventory = tools::call(&mut engine, "agent_inventory", json!({})).unwrap();
    let servers = inventory["mcp_servers"].as_array().unwrap();
    let search = servers
        .iter()
        .filter(|item| item["label"] == "search")
        .collect::<Vec<_>>();
    assert_eq!(search.len(), 2, "{}", dump(&inventory));
    assert_ne!(search[0]["id"], search[1]["id"]);
    let error = tools::call(&mut engine, "agent_trace", json!({"label": "search"})).unwrap_err();
    assert!(error.contains("ambiguous"), "{error}");
}

#[test]
fn portable_plugin_keeps_unknown_extensions_visible() {
    let (_fixture, mut engine) = engine();
    let inventory = tools::call(&mut engine, "agent_inventory", json!({})).unwrap();
    let plugins = dump(&inventory);
    assert!(plugins.contains("search-a"), "{plugins}");
    assert!(plugins.contains("com.example.client"), "{plugins}");
    let trace = tools::call(&mut engine, "agent_trace", json!({"label": "search-a"})).unwrap();
    let text = dump(&trace);
    assert!(text.contains("portable"), "{text}");
    assert!(text.contains("com.example.client"), "{text}");
}

#[test]
fn native_overlay_is_not_pretended_to_be_portable() {
    let (_fixture, mut engine) = engine();
    let inventory = tools::call(&mut engine, "agent_inventory", json!({})).unwrap();
    let plugins = inventory["plugins"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|item| item["label"] == "lookup")
        .collect::<Vec<_>>();
    assert_eq!(plugins.len(), 1, "{}", dump(&inventory));
    assert_eq!(plugins[0]["profile"], "cursor");
    assert_ne!(plugins[0]["profile"], "portable");
}

#[test]
fn escaped_command_is_invalid_and_not_executed() {
    let (_fixture, mut engine) = engine();
    let inventory = tools::call(&mut engine, "agent_inventory", json!({})).unwrap();
    let escaped = inventory["mcp_servers"]
        .as_array()
        .unwrap()
        .iter()
        .find(|item| item["label"] == "escaped")
        .expect("escaped server");
    assert_eq!(escaped["completeness"], "invalid");
    assert_eq!(inventory["bounds"]["executed"], false);
}

#[test]
fn secret_headers_do_not_reach_inventory_labels() {
    let (_fixture, mut engine) = engine();
    let inventory = tools::call(&mut engine, "agent_inventory", json!({})).unwrap();
    let text = dump(&inventory);
    assert!(!text.contains("secret-token"), "{text}");
    assert!(
        !text.to_ascii_lowercase().contains("bearer secret"),
        "{text}"
    );
}

#[test]
fn agent_profile_exposes_the_three_domain_tools() {
    let catalog = tools::catalog_for_profile(tools::ToolProfile::Agent);
    let names = catalog.iter().map(|tool| tool.name).collect::<Vec<_>>();
    assert!(names.contains(&"agent_inventory"));
    assert!(names.contains(&"agent_trace"));
    assert!(names.contains(&"agent_context"));
    assert!(!names.contains(&"verified_change"));
}
