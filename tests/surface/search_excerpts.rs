#![cfg(feature = "search")]

use crate::language_fixture::Fixture;
use blazingly_json::json;
use weavatrix_rust::{Weavatrix, tools};

#[test]
fn a_long_first_match_does_not_erase_later_search_hits() {
    let fixture = Fixture::new();
    fixture.write(
        "src/a.js",
        &format!("{}priority{}", "x".repeat(4_000), "y".repeat(4_000)),
    );
    fixture.write("src/b.js", "export const priority = 1;\n");
    let mut engine = Weavatrix::open(&fixture.root).unwrap();
    let result = tools::call(
        &mut engine,
        "search_code",
        json!({"query": "priority", "token_budget": 800}),
    )
    .unwrap();
    let hits = result["matches"].as_array().unwrap();
    assert_eq!(hits.len(), 2, "{result}");
    assert_eq!(hits[0]["path"], "src/a.js");
    assert_eq!(hits[0]["text_truncated"], true);
    assert!(hits[0]["text"].as_str().unwrap().contains("priority"));
    assert_eq!(hits[1]["path"], "src/b.js");
    assert_eq!(hits[1]["text_truncated"], false);

    let unbudgeted = tools::call(&mut engine, "search_code", json!({"query": "priority"})).unwrap();
    assert_eq!(
        unbudgeted["matches"][0]["text"].as_str().unwrap().len(),
        8_008
    );
    assert_eq!(unbudgeted["matches"][0]["text_truncated"], false);
}

#[test]
fn budgeted_search_excerpt_keeps_utf8_and_rebased_match_offsets() {
    let fixture = Fixture::new();
    fixture.write(
        "src/unicode.ts",
        &format!("{}priority{}", "é".repeat(300), "界".repeat(300)),
    );
    let mut engine = Weavatrix::open(&fixture.root).unwrap();
    let result = tools::call(
        &mut engine,
        "search_code",
        json!({"query": "priority", "token_budget": 800}),
    )
    .unwrap();
    let hit = &result["matches"][0];
    let text = hit["text"].as_str().unwrap();
    let start = usize::try_from(hit["spans"][0]["start"].as_u64().unwrap()).unwrap();
    let end = usize::try_from(hit["spans"][0]["end"].as_u64().unwrap()).unwrap();
    assert_eq!(&text[start..end], "priority");
    assert_eq!(hit["text_truncated"], true);
}
