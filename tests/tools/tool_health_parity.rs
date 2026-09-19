use crate::tool_fixture::Fixture;
use blazingly_json::json;
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

#[test]
fn audit_attaches_revision_bound_external_test_evidence() {
    let fixture = Fixture::new();
    fixture.write("src/index.js", "export const value = 1;\n");
    let mut engine = Weavatrix::open(&fixture.root).unwrap();
    let empty = tools::call(&mut engine, "run_audit", json!({})).unwrap();
    assert_eq!(empty["test_report"]["status"], "NOT_MEASURED");
    let revision = empty["repository_context"]["scan_revision"]
        .as_str()
        .unwrap()
        .to_owned();
    let passed = tools::call(
        &mut engine,
        "run_audit",
        json!({
            "test_evidence": {
                "schema": "weavatrix.test-evidence.v1",
                "repositoryRevision": revision,
                "success": true,
                "exitCode": 0
            }
        }),
    )
    .unwrap();
    assert_eq!(passed["test_report"]["status"], "PASS");
    let failed = tools::call(
        &mut engine,
        "run_audit",
        json!({
            "category": "tests",
            "test_evidence": {
                "schema": "weavatrix.test-evidence.v1",
                "repositoryRevision": passed["repository_context"]["scan_revision"],
                "success": false,
                "exitCode": 1
            }
        }),
    )
    .unwrap();
    assert_eq!(failed["test_report"]["status"], "FAIL");
    assert_eq!(failed["status"], "REVIEW");
    let skipped = tools::call(
        &mut engine,
        "run_audit",
        json!({"include_tests": false, "category": "tests"}),
    )
    .unwrap();
    assert_eq!(skipped["test_report"]["status"], "SKIPPED");
    assert!(
        tools::call(
            &mut engine,
            "run_audit",
            json!({
                "test_evidence": {"schema": "weavatrix.test-evidence.v1"},
                "test_evidence_path": "evidence.json"
            }),
        )
        .unwrap_err()
        .contains("mutually exclusive")
    );
    assert!(
        tools::call(
            &mut engine,
            "run_audit",
            json!({"test_evidence": {"schema": "other", "repositoryRevision": revision}}),
        )
        .unwrap_err()
        .contains("schema")
    );
    fixture.write(
        "evidence.json",
        &format!(
            "{{\"schema\":\"weavatrix.test-evidence.v1\",\"repositoryRevision\":\"{revision}\",\"success\":true,\"exitCode\":0}}"
        ),
    );
    let from_path = tools::call(
        &mut engine,
        "run_audit",
        json!({"test_evidence_path": "evidence.json"}),
    )
    .unwrap();
    assert_eq!(from_path["test_report"]["status"], "PASS");
    assert!(
        tools::call(
            &mut engine,
            "run_audit",
            json!({"test_evidence_path": "../outside.json"}),
        )
        .unwrap_err()
        .contains("inside the repository")
    );
}
