use crate::n8n_support::engine;
use weavatrix_rust::{Analyzer, SourceInput};

#[test]
fn nested_and_duplicate_expression_sites_keep_their_own_ranges() {
    let raw = include_str!("../fixtures/n8n/duplicate-text.json");
    let snapshot = Analyzer::default()
        .analyze_sources(
            std::env::current_dir().unwrap(),
            "dup",
            [SourceInput {
                path: "workflows/duplicate-text.json".into(),
                bytes: raw.as_bytes().to_vec(),
                content_hash: None,
            }],
        )
        .unwrap();
    let fields = snapshot
        .nodes
        .iter()
        .filter(|node| node.label.contains("customer.email"))
        .collect::<Vec<_>>();
    assert!(
        fields.len() >= 2,
        "nested selector segments must survive: {:?}",
        snapshot
            .nodes
            .iter()
            .map(|node| node.label.as_str())
            .collect::<Vec<_>>()
    );
    let first = snapshot
        .nodes
        .iter()
        .find(|node| node.label == "First Note")
        .and_then(|node| node.span.as_ref())
        .expect("first note span");
    let second = snapshot
        .nodes
        .iter()
        .find(|node| node.label == "Second Note")
        .and_then(|node| node.span.as_ref())
        .expect("second note span");
    assert!(
        first.start.line != second.start.line || first.start.column != second.start.column,
        "identical parameter text must not share the first occurrence"
    );
}

#[test]
fn two_node_refs_in_one_expression_stay_separate() {
    let (_fixture, engine) = engine();
    let labels = engine
        .state()
        .graph()
        .nodes()
        .iter()
        .map(|node| node.label.as_str())
        .collect::<Vec<_>>();
    assert!(
        labels.iter().any(|label| label.contains("Load Customer")
            && labels.iter().any(|other| other.contains("Check Customer"))),
        "{labels:?}"
    );
    let fields = labels
        .iter()
        .filter(|label| label.contains(".email") || label.contains(".name"))
        .count();
    assert!(fields >= 2, "{labels:?}");
}

#[test]
fn item_and_first_keep_distinct_selectors() {
    let (_fixture, engine) = engine();
    let labels = engine
        .state()
        .graph()
        .nodes()
        .iter()
        .map(|node| node.label.as_str())
        .collect::<Vec<_>>();
    assert!(
        labels.iter().any(|label| label.contains("linked-item")),
        "{labels:?}"
    );
    assert!(
        labels
            .iter()
            .any(|label| *label == "link:first" || label.ends_with("first")),
        "{labels:?}"
    );
}

#[test]
fn dynamic_node_name_stays_unresolved() {
    let (_fixture, engine) = engine();
    let names = engine
        .state()
        .graph()
        .nodes()
        .iter()
        .map(|node| node.label.as_str())
        .collect::<Vec<_>>();
    assert!(names.contains(&"unresolved:dynamic_node"), "{names:?}");
    assert!(
        names.iter().any(|name| name.contains("unresolved_key")),
        "{names:?}"
    );
}

#[test]
fn unicode_and_json_escapes_keep_a_raw_locator() {
    let raw = include_str!("../fixtures/n8n/unicode-escape.json");
    let snapshot = Analyzer::default()
        .analyze_sources(
            std::env::current_dir().unwrap(),
            "unicode",
            [SourceInput {
                path: "workflows/unicode-escape.json".into(),
                bytes: raw.as_bytes().to_vec(),
                content_hash: None,
            }],
        )
        .unwrap();
    let write_note = snapshot
        .nodes
        .iter()
        .find(|node| node.label == "Write Note")
        .unwrap();
    let span = write_note.span.as_ref().expect("source span");
    assert_eq!(span.file, "workflows/unicode-escape.json");
    assert!(span.start.line >= 1);
    assert!(raw.contains(r"\u0040"), "fixture must keep the JSON escape");
}

#[test]
fn langchain_type_version_is_not_coerced_to_int() {
    let (_fixture, engine) = engine();
    assert!(
        engine
            .state()
            .graph()
            .nodes()
            .iter()
            .any(|node| node.label.contains("1.7")),
        "typeVersion 1.7 must remain a domain label"
    );
    assert!(
        engine
            .state()
            .graph()
            .nodes()
            .iter()
            .any(|node| node.label.contains("ai_languageModel")),
        "AI connections stay as configured roles, not execution flow"
    );
}
