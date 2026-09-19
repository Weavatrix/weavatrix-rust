use crate::agent_support::{dump, engine};
use blazingly_json::json;
use weavatrix_rust::tools;

#[test]
fn skill_inventory_keeps_package_scoped_names() {
    let (_fixture, mut engine) = engine();
    let inventory = tools::call(&mut engine, "agent_inventory", json!({})).unwrap();
    let skills = inventory["skills"].as_array().unwrap();
    assert!(
        skills.iter().any(|item| item["label"] == "summarize"),
        "{}",
        dump(&inventory)
    );
    assert!(
        skills.iter().any(|item| item["label"] == "lookup"),
        "{}",
        dump(&inventory)
    );
}

#[test]
fn allowed_tools_stay_declared_and_are_not_a_grant() {
    let (_fixture, mut engine) = engine();
    let inventory = tools::call(&mut engine, "agent_inventory", json!({})).unwrap();
    let skill_id = inventory["skills"]
        .as_array()
        .unwrap()
        .iter()
        .find(|item| item["label"] == "lookup")
        .and_then(|item| item["id"].as_str())
        .expect("lookup skill")
        .to_owned();
    let context = tools::call(
        &mut engine,
        "agent_context",
        json!({"label": skill_id, "task": "inspect"}),
    )
    .unwrap();
    let text = dump(&context);
    assert!(
        text.contains("declared_allowed_tools:Read") || text.contains("Read"),
        "{text}"
    );
    assert_eq!(context["bounds"]["authorization"], "none");
    assert_eq!(context["bounds"]["executed"], false);
    assert!(
        text.contains("no grant") || text.contains("declared only"),
        "{text}"
    );
}

#[test]
fn skill_markdown_references_are_documentary() {
    let (_fixture, mut engine) = engine();
    let trace = tools::call(&mut engine, "agent_trace", json!({"label": "summarize"})).unwrap();
    let text = dump(&trace);
    assert!(text.contains("references/guide.md"), "{text}");
}

#[test]
fn readme_is_not_a_skill() {
    let (_fixture, engine) = engine();
    assert!(
        engine
            .state()
            .graph()
            .nodes()
            .iter()
            .filter(|node| node.kind.as_str() == "agent.skill")
            .all(|node| node.label != "README")
    );
}

#[test]
fn inventory_max_results_reports_the_cut() {
    let (_fixture, mut engine) = engine();
    let inventory = tools::call(&mut engine, "agent_inventory", json!({"max_results": 1})).unwrap();
    assert_eq!(inventory["plugins"].as_array().unwrap().len(), 1);
    assert_eq!(inventory["bounds"]["truncated"], true);
    assert!(inventory["bounds"]["found"].as_u64().unwrap() > 1);
}
