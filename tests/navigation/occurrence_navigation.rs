//! Position navigation resolves a usage, not a same-named declaration.

use crate::language_fixture::Fixture;
use blazingly_json::{Value, json};
use weavatrix_rust::{EdgeKind, Weavatrix, operations};

const ALPHA: &str = "export function parse(value) { return value; }\n";
const BETA: &str = "export function parse(value) { return value + 1; }\n";
const APP: &str =
    "import { parse } from './alpha.js';\nexport function run() { return parse(1); }\n";

fn same_name_repo() -> Fixture {
    let fixture = Fixture::new();
    fixture.write("src/alpha.js", ALPHA);
    fixture.write("src/beta.js", BETA);
    fixture.write("src/app.js", APP);
    fixture
}

fn call_engine(fixture: &Fixture) -> Weavatrix {
    Weavatrix::open(&fixture.root).unwrap()
}

fn call(engine: &mut Weavatrix, name: &str, args: Value) -> Value {
    operations::call(engine, name, args).unwrap_or_else(|error| panic!("{name}: {error}"))
}

fn usage_of(engine: &Weavatrix, label: &str) -> (String, u32, u32) {
    let target = engine
        .state()
        .graph()
        .nodes()
        .iter()
        .find(|node| {
            node.label == label
                && node
                    .span
                    .as_ref()
                    .is_some_and(|span| span.file.contains("alpha"))
        })
        .unwrap_or_else(|| panic!("definition {label}"));
    let span = engine
        .state()
        .graph()
        .edges()
        .iter()
        .find(|edge| edge.kind == EdgeKind::Calls && edge.target == target.id)
        .and_then(|edge| edge.provenance.span.as_ref())
        .unwrap_or_else(|| panic!("call to {label}"));
    (span.file.clone(), span.start.line, span.start.column)
}

fn identifier(source: &str, token: &str) -> (u32, u32) {
    for (index, line) in source.lines().enumerate() {
        if let Some(column) = line.find(token) {
            return (
                u32::try_from(index).expect("line") + 1,
                u32::try_from(column).expect("column") + 1,
            );
        }
    }
    panic!("{token} not found");
}

#[test]
fn a_usage_resolves_to_the_imported_definition_not_the_same_named_overload() {
    let fixture = same_name_repo();
    let mut engine = call_engine(&fixture);
    let (path, line, column) = usage_of(&engine, "parse");
    let report = call(
        &mut engine,
        "go_to_definition",
        json!({"path": path, "line": line, "column": column}),
    );

    assert_eq!(report["state"], "RESOLVED", "{report}");
    assert_eq!(report["definition"]["label"], "parse");
    assert_eq!(report["definition"]["span"]["file"], "src/alpha.js");
    assert_ne!(
        report["definition"]["span"]["file"], "src/beta.js",
        "the other parse must not win"
    );
}

#[test]
fn inspect_by_name_stays_ambiguous_when_position_is_exact() {
    let fixture = same_name_repo();
    let mut engine = call_engine(&fixture);
    let error = operations::call(&mut engine, "inspect_symbol", json!({"label": "parse"}))
        .expect_err("two parse declarations");
    assert!(error.contains("ambiguous"), "{error}");

    let (path, line, column) = usage_of(&engine, "parse");
    let report = call(
        &mut engine,
        "inspect_symbol",
        json!({"path": path, "line": line, "column": column}),
    );
    assert_eq!(report["state"], "RESOLVED", "{report}");
    assert_eq!(report["node"]["span"]["file"], "src/alpha.js");
}

#[test]
fn find_references_from_a_usage_includes_the_definition_and_the_call() {
    let fixture = same_name_repo();
    let mut engine = call_engine(&fixture);
    let (path, line, column) = usage_of(&engine, "parse");
    let report = call(
        &mut engine,
        "find_references",
        json!({"path": path, "line": line, "column": column}),
    );
    assert_eq!(report["state"], "RESOLVED", "{report}");
    let roles = report["references"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|item| item["role"].as_str())
        .collect::<Vec<_>>();
    assert!(roles.contains(&"definition"), "{report}");
    assert!(roles.contains(&"reference"), "{report}");
    assert_eq!(report["definition"]["span"]["file"], "src/alpha.js");
}

#[test]
fn a_position_without_an_occurrence_stays_unresolved() {
    let fixture = same_name_repo();
    let mut engine = call_engine(&fixture);
    let report = call(
        &mut engine,
        "go_to_definition",
        json!({"path": "src/app.js", "line": 1, "column": 1}),
    );
    assert_eq!(report["state"], "UNRESOLVED", "{report}");
    assert!(report["definition"].is_null());
    assert_eq!(report["reason"], "no occurrence at this position");
}

#[test]
fn standing_on_a_declaration_name_is_that_definition() {
    let fixture = same_name_repo();
    let mut engine = call_engine(&fixture);
    let (line, column) = identifier(ALPHA, "parse");
    let report = call(
        &mut engine,
        "go_to_definition",
        json!({"path": "src/alpha.js", "line": line, "column": column}),
    );
    assert_eq!(report["state"], "RESOLVED", "{report}");
    assert_eq!(report["occurrence"]["role"], "definition");
    assert_eq!(report["definition"]["span"]["file"], "src/alpha.js");
}

#[cfg(feature = "lang-rust")]
#[test]
fn an_unresolved_method_call_does_not_bind_a_same_named_free_function() {
    const SOURCE: &str = "pub fn push(value: u32) -> u32 { value }\n\
         pub fn run(values: &mut Vec<u32>) { values.push(9); }\n";
    let fixture = Fixture::new();
    fixture.write("src/lib.rs", SOURCE);
    let mut engine = call_engine(&fixture);
    let (line, column) = identifier(SOURCE, ".push");
    let column = column + 1;
    let report = call(
        &mut engine,
        "go_to_definition",
        json!({"path": "src/lib.rs", "line": line, "column": column}),
    );
    assert_eq!(report["state"], "UNRESOLVED", "{report}");
    assert!(report["definition"].is_null());
}
