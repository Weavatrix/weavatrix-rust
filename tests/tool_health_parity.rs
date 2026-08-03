mod tool_fixture;

use blazingly_json::json;
use tool_fixture::Fixture;
use weavatrix_rust::{Weavatrix, tools};

#[test]
fn audit_filters_match_the_advertised_schema() {
    let fixture = Fixture::new();
    fixture.write(
        "package.json",
        "{\"name\":\"fixture\",\"version\":\"1.0.0\",\"dependencies\":{\"unused\":\"1.0.0\"}}\n",
    );
    fixture.write("src/index.js", "export const value = 1;\n");
    let mut engine = Weavatrix::open(&fixture.root).unwrap();
    let definition = tools::catalog()
        .into_iter()
        .find(|tool| tool.name == "run_audit")
        .unwrap();

    assert_eq!(
        definition.input_schema["properties"]["min_severity"]["type"],
        "string"
    );
    let filtered = tools::call(
        &mut engine,
        "run_audit",
        json!({"category": "dependencies", "min_severity": "medium"}),
    )
    .unwrap();
    assert_eq!(filtered["runtime_report"]["findings_total"], 0);
    assert_eq!(filtered["dependency_report"]["findings_total"], 0);
    assert!(
        tools::call(&mut engine, "run_audit", json!({"min_severity": 2}))
            .unwrap_err()
            .contains("min_severity must be a string")
    );
}

#[test]
#[cfg(feature = "clone")]
fn duplicate_threshold_accepts_fraction_and_percent_forms() {
    let fixture = Fixture::new();
    fixture.write(
        "src/left.js",
        "export function left(value) {\n  if (value > 10) {\n    return value * 2;\n  }\n  return value + 1;\n}\n",
    );
    fixture.write(
        "src/right.js",
        "export function right(value) {\n  if (value > 10) {\n    return value * 2;\n  }\n  return value + 1;\n}\n",
    );
    let mut engine = Weavatrix::open(&fixture.root).unwrap();
    let fraction = tools::call(
        &mut engine,
        "find_duplicates",
        json!({"min_tokens": 12, "min_similarity": 0.92}),
    )
    .unwrap();
    let percent = tools::call(
        &mut engine,
        "find_duplicates",
        json!({"min_tokens": 12, "min_similarity": 92}),
    )
    .unwrap();

    assert_eq!(fraction["families"], percent["families"]);
    assert_eq!(fraction["pairs"], percent["pairs"]);
}

#[test]
fn rust_tarpaulin_coverage_is_measured_not_static_reachability() {
    let fixture = Fixture::new();
    fixture.write("src/lib.rs", "pub fn covered() {}\n");
    fixture.write(
        "tarpaulin-report.json",
        r#"{
          "files": [{
            "path": ["src", "lib.rs"],
            "content": "pub fn covered() {}",
            "traces": [],
            "covered": 1,
            "coverable": 1
          }],
          "coverage": 100.0,
          "covered": 1,
          "coverable": 1
        }"#,
    );
    let mut engine = Weavatrix::open(&fixture.root).unwrap();
    let coverage = tools::call(&mut engine, "coverage_map", json!({})).unwrap();
    assert_eq!(coverage["status"], "COMPLETE");
    assert_eq!(coverage["measured_coverage"]["present"], true);
    assert_eq!(coverage["files"][0]["lines_hit"], 1);
    assert_eq!(coverage["files"][0]["lines_found"], 1);
}

#[test]
fn static_coverage_counts_test_suites_without_counting_jest_support_files() {
    let fixture = Fixture::new();
    fixture.write("src/index.js", "export const value = 1;\n");
    fixture.write("tests/unit/alpha.test.js", "test('alpha', () => {});\n");
    fixture.write("tests/unit/beta.spec.js", "test('beta', () => {});\n");
    fixture.write("tests/__tests__/gamma.js", "test('gamma', () => {});\n");
    fixture.write(
        "tests/configurations/testSetup.js",
        "export const setup = true;\n",
    );
    fixture.write(
        "tests/configurations/testTeardown.js",
        "export const teardown = true;\n",
    );
    fixture.write("tests/helpers.js", "export const helper = true;\n");

    let mut engine = Weavatrix::open(&fixture.root).unwrap();
    let coverage = tools::call(&mut engine, "coverage_map", json!({})).unwrap();

    assert_eq!(coverage["measured_coverage"]["present"], false);
    assert_eq!(
        coverage["static_reachability"]["test_files"], 3,
        "only Jest suites, not setup, teardown, or helper modules, are test files"
    );
}
