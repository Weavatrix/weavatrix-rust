mod dify_support;

use dify_support::engine;
use weavatrix_rust::{Analyzer, SourceInput};

fn labels_of(engine: &weavatrix_rust::Weavatrix) -> Vec<String> {
    engine
        .state()
        .graph()
        .nodes()
        .iter()
        .map(|node| node.label.clone())
        .collect()
}

#[test]
fn conversation_read_and_write_stay_typed() {
    let (_fixture, engine) = engine();
    let labels = labels_of(&engine);
    assert!(
        labels
            .iter()
            .any(|label| label == "conversation:customer_email"),
        "{labels:?}"
    );
    assert!(
        labels
            .iter()
            .any(|label| label.contains("selector:start_node.email")
                || label.contains("selector:conversation.customer_email")),
        "{labels:?}"
    );
}

#[test]
fn iteration_children_keep_parent_scope() {
    let (_fixture, engine) = engine();
    let labels = labels_of(&engine);
    assert!(
        labels
            .iter()
            .any(|label| label == "parent:each_item"
                || label.starts_with("scope:iteration:each_item")),
        "{labels:?}"
    );
    assert!(
        labels.iter().any(|label| label == "Format Item"),
        "{labels:?}"
    );
    assert!(
        labels.iter().any(|label| label == "binding:item"),
        "{labels:?}"
    );
}

#[test]
fn env_model_is_recorded_and_secrets_are_omitted() {
    let (_fixture, mut engine) = engine();
    let labels = labels_of(&engine);
    assert!(
        labels
            .iter()
            .any(|label| label.contains("model:env:CHAT_MODEL")),
        "{labels:?}"
    );
    assert!(
        labels.iter().any(|label| label == "env:CHAT_MODEL"),
        "{labels:?}"
    );
    assert!(
        labels
            .iter()
            .all(|label| !label.contains("sk-not-a-real-key")),
        "{labels:?}"
    );
    let context = weavatrix_rust::tools::call(
        &mut engine,
        "dify_context",
        blazingly_json::json!({"label": "Env LLM"}),
    )
    .unwrap();
    let text = blazingly_json::to_string(&context).unwrap();
    assert!(!text.contains("sk-not-a-real-key"), "{text}");
    assert!(!text.to_ascii_lowercase().contains("openai_api_key") || text.contains("env:redacted"));
}

#[test]
fn unicode_title_keeps_a_raw_locator() {
    let raw = include_str!("fixtures/dify/unicode-title.yml");
    let snapshot = Analyzer::default()
        .analyze_sources(
            std::env::current_dir().unwrap(),
            "unicode",
            [SourceInput {
                path: "apps/unicode-title.yml".into(),
                bytes: raw.as_bytes().to_vec(),
                content_hash: None,
            }],
        )
        .unwrap();
    let labels = snapshot
        .nodes
        .iter()
        .map(|node| node.label.as_str())
        .collect::<Vec<_>>();
    assert!(
        labels
            .iter()
            .any(|label| label.contains("Привет") || label.contains("Узел")),
        "{labels:?}"
    );
}

#[test]
fn code_declarations_are_inventory_not_execution() {
    let (_fixture, engine) = engine();
    let labels = labels_of(&engine);
    assert!(
        labels.iter().any(|label| label.contains("code:decl:main")),
        "{labels:?}"
    );
    assert!(
        labels.iter().any(|label| label == "output:result"),
        "{labels:?}"
    );
}
