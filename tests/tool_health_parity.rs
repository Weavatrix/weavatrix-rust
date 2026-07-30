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
fn audit_compares_external_imports_with_supported_manifests() {
    let fixture = Fixture::new();
    fixture.write("package.json", r#"{"dependencies":{"lodash":"1.0.0"}}"#);
    fixture.write(
        "src/server.ts",
        "import express from \"express\";\nexport const app = express();\n",
    );
    let mut engine = Weavatrix::open(&fixture.root).unwrap();
    let audit = tools::call(&mut engine, "run_audit", json!({})).unwrap();
    let findings = audit["dependency_report"]["findings"].as_array().unwrap();
    assert!(
        findings
            .iter()
            .any(|finding| finding["id"] == "dependency.missing:typescript:express")
    );
    assert!(findings.iter().any(|finding| {
        finding["rule"] == "dependency.unused_declaration" && finding["package"] == "lodash"
    }));
}

#[test]
#[cfg(feature = "lang-rust")]
fn audit_matches_hyphenated_cargo_dependencies_to_grouped_rust_uses() {
    let fixture = Fixture::new();
    fixture.write(
        "Cargo.toml",
        "[package]\nname = \"fixture\"\nversion = \"0.1.0\"\n\n[dependencies]\nblazingly-json = \"0.1.0\"\n",
    );
    fixture.write(
        "src/lib.rs",
        "use blazingly_json::{Value, json};\npub fn value() -> Value { json!({\"ok\": true}) }\n",
    );

    let mut engine = Weavatrix::open(&fixture.root).unwrap();
    let audit = tools::call(&mut engine, "run_audit", json!({})).unwrap();
    let findings = audit["dependency_report"]["findings"].as_array().unwrap();

    assert!(
        !findings.iter().any(|finding| {
            finding["rule"] == "dependency.unused_declaration"
                && finding["package"] == "blazingly-json"
        }),
        "grouped use must count as exact Cargo import evidence, got {findings:?}"
    );
}

#[test]
#[cfg(feature = "lang-rust")]
fn audit_uses_only_production_import_evidence() {
    let fixture = Fixture::new();
    fixture.write(
        "Cargo.toml",
        "[package]\nname = \"fixture\"\nversion = \"0.1.0\"\n\n[dev-dependencies]\ndev-only = \"1\"\nbench-only = \"1\"\nfuzz-only = \"1\"\ninline-dev = \"1\"\n",
    );
    fixture.write(
        "src/lib.rs",
        "use missing_prod::item;\npub fn value() { let _ = item(); }\n\
         #[cfg(test)]\nmod tests {\n    use inline_dev::helper;\n    #[test]\n    fn check() { helper(); }\n}\n",
    );
    fixture.write(
        "tests/integration.rs",
        "use dev_only::helper;\n#[test]\nfn check() { helper(); }\n",
    );
    fixture.write(
        "benches/load.rs",
        "#[path = \"support/scale_harness.rs\"]\nmod scale_harness;\n\
         use bench_only::run;\nuse scale_harness::measure;\nfn main() { run(); measure(); }\n",
    );
    fixture.write("benches/support/scale_harness.rs", "pub fn measure() {}\n");
    fixture.write(
        "fuzz/fuzz_targets/parser.rs",
        "use fuzz_only::run;\nfn main() { run(); }\n",
    );
    fixture.write(
        "fuzz/Cargo.toml",
        "[package]\nname = \"fixture-fuzz\"\nversion = \"0.1.0\"\n\n[dependencies]\nlibfuzzer-sys = \"0.4\"\nfuzz-only = \"1\"\n",
    );

    let mut engine = Weavatrix::open(&fixture.root).unwrap();
    let packages = engine
        .state()
        .graph()
        .nodes()
        .iter()
        .filter(|node| node.kind == weavatrix_graph::NodeKind::Package)
        .map(|node| node.label.as_str())
        .collect::<std::collections::BTreeSet<_>>();
    for expected in [
        "missing_prod",
        "inline_dev",
        "dev_only",
        "bench_only",
        "scale_harness",
        "fuzz_only",
    ] {
        assert!(
            packages.contains(expected),
            "fixture must exercise package evidence for {expected}: {packages:?}"
        );
    }

    let audit = tools::call(&mut engine, "run_audit", json!({})).unwrap();
    let findings = audit["dependency_report"]["findings"].as_array().unwrap();
    assert!(findings.iter().any(|finding| {
        finding["rule"] == "dependency.missing_declaration" && finding["package"] == "missing_prod"
    }));
    for excluded in [
        "inline_dev",
        "dev_only",
        "bench_only",
        "scale_harness",
        "fuzz_only",
        "libfuzzer-sys",
    ] {
        assert!(
            !findings
                .iter()
                .any(|finding| finding["package"] == excluded),
            "{excluded} is non-production dependency evidence: {findings:?}"
        );
    }
}
