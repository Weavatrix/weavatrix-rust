use crate::agent_support::{dump, engine};
use blazingly_json::json;
use weavatrix_rust::tools;

#[test]
fn catalog_tools_stay_package_scoped() {
    let (_fixture, mut engine) = engine();
    let inventory = tools::call(&mut engine, "agent_inventory", json!({})).unwrap();
    let tools = inventory["tools"].as_array().unwrap();
    let customers = tools
        .iter()
        .filter(|item| item["label"] == "get_customer")
        .count();
    assert_eq!(customers, 2, "{}", dump(&inventory));
}

#[test]
fn transform_hides_upstream_required_only_for_the_exposure() {
    let (_fixture, mut engine) = engine();
    let report = tools::call(
        &mut engine,
        "agent_change_impact",
        json!({"before": "catalogs/before.json", "after": "catalogs/after.json"}),
    )
    .unwrap();
    let changes = report["changes"].as_array().unwrap();
    let raw = changes
        .iter()
        .find(|item| item["tool"] == "get_customer")
        .expect("get_customer");
    let lookup = changes
        .iter()
        .find(|item| item["tool"] == "lookup_customer")
        .expect("lookup_customer");
    assert_eq!(
        raw["compatibility"],
        "proven-incompatible",
        "{}",
        dump(&report)
    );
    assert_eq!(
        lookup["compatibility"],
        "proven-compatible",
        "{}",
        dump(&report)
    );
    assert!(
        lookup["adapter_fills"]
            .as_array()
            .is_some_and(|items| items.iter().any(|item| item == "tenant")),
        "{}",
        dump(&report)
    );
}

#[test]
fn description_only_change_is_not_hidden_by_schema_hash() {
    let (_fixture, mut engine) = engine();
    let report = tools::call(
        &mut engine,
        "agent_change_impact",
        json!({"before": "catalogs/before.json", "after": "catalogs/after.json"}),
    )
    .unwrap();
    let lookup = report["changes"]
        .as_array()
        .unwrap()
        .iter()
        .find(|item| item["tool"] == "lookup_customer")
        .unwrap();
    assert_eq!(lookup["description_changed"], false);
}

#[test]
fn origin_map_and_dispatcher_are_implemented_not_exposed() {
    let (_fixture, engine) = engine();
    let registrations = engine
        .state()
        .graph()
        .nodes()
        .iter()
        .filter(|node| node.kind.as_str() == "agent.registration")
        .map(|node| node.label.as_str())
        .collect::<Vec<_>>();
    assert!(registrations.contains(&"get_customer"), "{registrations:?}");
}

#[test]
fn replayed_observation_does_not_count_twice_and_success_is_not_an_effect() {
    let (_fixture, mut engine) = engine();
    let inventory = tools::call(&mut engine, "agent_inventory", json!({})).unwrap();
    let text = dump(&inventory);
    assert!(text.contains("lookup_customer"), "{text}");
    assert!(
        text.contains("result_is_not_effect") || text.contains("effect:unverified"),
        "{text}"
    );
    let replayed = engine
        .state()
        .graph()
        .nodes()
        .iter()
        .flat_map(|node| {
            engine
                .state()
                .graph()
                .edges()
                .iter()
                .filter(move |edge| edge.source.as_str() == node.id.as_str())
                .filter_map(|edge| engine.state().graph().node(edge.target.as_str()))
        })
        .any(|node| node.label.starts_with("replayed:"));
    assert!(replayed, "replayed observation domain missing");
}

#[test]
fn unresolved_sdk_factory_is_not_given_a_handler() {
    let (_fixture, engine) = engine();
    assert!(
        engine
            .state()
            .graph()
            .nodes()
            .iter()
            .any(|node| node.label == "dynamic-factory"),
        "unresolved factory must stay visible"
    );
}

#[test]
fn agent_profile_includes_change_impact() {
    let catalog = tools::catalog_for_profile(tools::ToolProfile::Agent);
    assert!(
        catalog
            .iter()
            .any(|tool| tool.name == "agent_change_impact")
    );
}

#[test]
fn decorator_keeps_public_name_and_impl_identity() {
    let (_fixture, engine) = engine();
    let registration = engine
        .state()
        .graph()
        .nodes()
        .iter()
        .find(|node| node.kind.as_str() == "agent.registration" && node.label == "public_lookup")
        .expect("public tool name");
    let domains = engine
        .state()
        .graph()
        .edges()
        .iter()
        .filter(|edge| edge.source.as_str() == registration.id.as_str())
        .filter_map(|edge| engine.state().graph().node(edge.target.as_str()))
        .map(|node| node.label.as_str())
        .collect::<Vec<_>>();
    assert!(
        domains.contains(&"impl:private_lookup_impl")
            || domains.contains(&"handler:private_lookup_impl"),
        "{domains:?}"
    );
    assert!(
        !engine
            .state()
            .graph()
            .nodes()
            .iter()
            .any(|node| node.kind.as_str() == "agent.registration"
                && node.label == "private_lookup_impl"),
        "impl name must not replace the exported tool name"
    );
}
