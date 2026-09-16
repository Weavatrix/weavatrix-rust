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
    let graph = engine.state().graph();
    let remember = graph
        .nodes()
        .iter()
        .find(|node| node.label == "Remember Email")
        .expect("assigner");
    let variable = graph
        .nodes()
        .iter()
        .find(|node| {
            node.label == "conversation:customer_email" && node.id.as_str().starts_with("symbol:")
        })
        .expect("conversation variable");
    let start = graph
        .nodes()
        .iter()
        .find(|node| {
            node.label == "Start"
                && node
                    .span
                    .as_ref()
                    .is_some_and(|span| span.file.contains("conversation-assign"))
        })
        .expect("start");
    let writes = graph
        .edges()
        .iter()
        .filter(|edge| {
            edge.source.as_str() == remember.id.as_str()
                && edge.target.as_str() == variable.id.as_str()
                && edge.kind.as_str() == "writes_variable"
        })
        .count();
    let reads = graph
        .edges()
        .iter()
        .filter(|edge| {
            edge.source.as_str() == remember.id.as_str()
                && edge.target.as_str() == start.id.as_str()
                && edge.kind.as_str() == "reads_variable"
        })
        .count();
    assert_eq!(
        writes, 1,
        "assigner must write the scoped conversation variable"
    );
    assert_eq!(reads, 1, "assigner must read start_node.email");
}

#[test]
fn assigner_v2_items_write_the_same_conversation_variable() {
    let (_fixture, engine) = engine();
    let graph = engine.state().graph();
    let remember = graph
        .nodes()
        .iter()
        .find(|node| node.label == "Remember")
        .expect("v2 assigner");
    let variable = graph
        .nodes()
        .iter()
        .find(|node| {
            node.label == "conversation:message" && node.id.as_str().starts_with("symbol:")
        })
        .expect("v2 conversation variable");
    let answer = graph
        .nodes()
        .iter()
        .find(|node| node.label == "Answer")
        .expect("answer");
    assert!(
        graph.edges().iter().any(|edge| {
            edge.source.as_str() == remember.id.as_str()
                && edge.target.as_str() == variable.id.as_str()
                && edge.kind.as_str() == "writes_variable"
        }),
        "v2 items[] must write conversation.message"
    );
    assert!(
        graph.edges().iter().any(|edge| {
            edge.source.as_str() == answer.id.as_str()
                && edge.target.as_str() == variable.id.as_str()
        }),
        "answer must consume the same variable"
    );
    assert!(
        graph
            .nodes()
            .iter()
            .all(|node| node.label != "marker:start_node.documentary"),
        "description markers stay documentary"
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
