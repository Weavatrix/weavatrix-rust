//! The context bundle never trades its subject away for its periphery.

use crate::language_fixture::Fixture;
use blazingly_json::json;
use std::fmt::Write as _;
use weavatrix_rust::{Weavatrix, tools};

fn hub_fixture() -> Fixture {
    let fixture = Fixture::new();
    let mut big = String::new();
    for index in 0..120 {
        let _ = writeln!(
            big,
            "import {{ hub }} from './hub.js';\nexport function caller{index}() {{ return hub(); }}"
        );
    }
    fixture.write("src/big.js", &big);
    fixture.write("src/hub.js", "export function hub() {\n  return 42;\n}\n");
    fixture
}

/// The repo-lens regression: a small budget deleted the target's own source
/// and kept a large relationships array. The trim order is relationships and
/// related source first; the target survives every budget the answer accepts.
#[test]
fn a_budgeted_bundle_keeps_the_target_source_and_trims_the_periphery() {
    let fixture = hub_fixture();
    let mut engine = Weavatrix::open(&fixture.root).unwrap();
    let report = tools::call(
        &mut engine,
        "context_bundle",
        json!({"label": "hub", "token_budget": 700}),
    )
    .unwrap();

    let lines = report["inspection"]["source"]["lines"].as_array().unwrap();
    assert!(
        !lines.is_empty(),
        "the target symbol's own source must survive the budget: {report}"
    );
    assert_eq!(
        report["token_budget"]["fit"], true,
        "after trimming the periphery the answer must fit: {report}"
    );
    let neighbors = report["inspection"]["relationships"]["neighbors"]
        .as_array()
        .unwrap();
    assert_eq!(
        report["inspection"]["relationships"]["page"]["returned"],
        neighbors.len() as u64,
        "pagination must describe the trimmed array, not the pre-trim one: {report}"
    );
}

/// A budget below the target symbol itself is an explicit caller error, not a
/// silent fit:false with the subject deleted.
#[test]
fn a_budget_below_the_target_itself_is_an_explicit_error() {
    let fixture = hub_fixture();
    let mut engine = Weavatrix::open(&fixture.root).unwrap();
    let error = tools::call(
        &mut engine,
        "context_bundle",
        json!({"label": "hub", "token_budget": 40}),
    )
    .unwrap_err();
    assert!(
        error.contains("token_budget") && error.contains("target"),
        "the error must name the budget and the target: {error}"
    );
}

/// `max_references` caps the relationships array, which was previously
/// advertised and ignored.
#[test]
fn max_references_caps_the_relationship_evidence() {
    let fixture = hub_fixture();
    let mut engine = Weavatrix::open(&fixture.root).unwrap();
    let report = tools::call(
        &mut engine,
        "context_bundle",
        json!({"label": "hub", "max_references": 7}),
    )
    .unwrap();
    let neighbors = report["inspection"]["relationships"]["neighbors"]
        .as_array()
        .unwrap();
    assert_eq!(
        neighbors.len(),
        7,
        "max_references must cap the returned relationships: {report}"
    );
    let error = tools::call(
        &mut engine,
        "inspect_symbol",
        json!({"label": "hub", "max_references": 0}),
    )
    .unwrap_err();
    assert!(
        error.contains("max_references"),
        "an out-of-range cap is a caller error: {error}"
    );
}

#[test]
fn related_source_keeps_a_callee_when_callers_dominate() {
    let fixture = Fixture::new();
    fixture.write(
        "src/hub.js",
        "import { helper } from './helper.js';\nexport function hub() { return helper(); }\n",
    );
    fixture.write("src/helper.js", "export function helper() { return 1; }\n");
    let mut callers = String::new();
    for index in 0..50 {
        let _ = writeln!(
            callers,
            "import {{ hub }} from './hub.js';\nexport function caller{index}() {{ return hub(); }}"
        );
    }
    fixture.write("src/callers.js", &callers);
    let mut engine = Weavatrix::open(&fixture.root).unwrap();
    let report = tools::call(
        &mut engine,
        "context_bundle",
        json!({"label": "hub", "max_related": 8}),
    )
    .unwrap();
    let categories = report["related_categories"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|item| item["category"].as_str())
        .collect::<Vec<_>>();
    assert!(
        categories.contains(&"callee"),
        "a single callee must survive a large caller fan-in: {report}"
    );
    let ids = report["related_categories"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|item| item["id"].as_str())
        .collect::<Vec<_>>();
    let unique = ids.iter().collect::<std::collections::BTreeSet<_>>();
    assert_eq!(
        unique.len(),
        ids.len(),
        "one symbol must appear once even if several edges point at it: {ids:?}"
    );
}
