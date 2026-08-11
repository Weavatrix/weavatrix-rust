#![cfg(feature = "lang-rust")]

mod language_fixture;

use blazingly_json::json;
use language_fixture::Fixture;
use std::fmt::Write;
use weavatrix_rust::{Weavatrix, tools};

#[test]
fn read_source_by_symbol_keeps_the_entire_named_definition() {
    let fixture = Fixture::new();
    let mut source = String::from("// archive configuration\npub struct ArchiveOptions {\n");
    for field in 0..48 {
        writeln!(source, "    field_{field}: usize,").unwrap();
    }
    source.push_str("}\npub fn after_definition() {}\n");
    fixture.write("src/lib.rs", &source);
    let mut engine = Weavatrix::open(&fixture.root).unwrap();

    let excerpt = tools::call(
        &mut engine,
        "read_source",
        json!({"label": "ArchiveOptions", "before": 0, "after": 0}),
    )
    .unwrap();
    let lines = excerpt["lines"].as_array().unwrap();

    assert_eq!(excerpt["start_line"], 2);
    assert_eq!(excerpt["end_line"], 51);
    assert_eq!(lines.last().unwrap()["text"], "}");
    assert!(
        lines
            .iter()
            .any(|line| line["text"] == "    field_47: usize,"),
        "the tail of the requested definition must not be cut off: {excerpt:?}"
    );
    assert!(
        lines
            .iter()
            .all(|line| line["text"] != "pub fn after_definition() {}"),
        "the symbol extent must stop at its own closing brace"
    );
}
