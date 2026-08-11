mod tool_fixture;

use blazingly_json::json;
use tool_fixture::Fixture;
use weavatrix_rust::{Weavatrix, tools};

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
fn audit_recognizes_bare_and_prefixed_node_builtins() {
    let fixture = Fixture::new();
    fixture.write(
        "src/server.js",
        "import fs from 'fs';\n\
         import promises from 'fs/promises';\n\
         import os from 'os';\n\
         import stream from 'stream';\n\
         import assert from 'node:assert/strict';\n\
         import missing from 'node:not-a-real-builtin';\n\
         export const values = [fs, promises, os, stream, assert, missing];\n",
    );

    let mut engine = Weavatrix::open(&fixture.root).unwrap();
    let audit = tools::call(
        &mut engine,
        "run_audit",
        json!({"category": "dependencies"}),
    )
    .unwrap();
    let findings = audit["dependency_report"]["findings"].as_array().unwrap();

    for builtin in ["fs", "os", "stream", "node:assert"] {
        assert!(
            !findings.iter().any(|finding| finding["package"] == builtin),
            "Node builtin {builtin} must not be reported as missing: {findings:?}"
        );
    }
    assert!(
        findings
            .iter()
            .any(|finding| finding["package"] == "node:not-a-real-builtin"),
        "unknown node: specifiers must not be accepted merely by prefix: {findings:?}"
    );
}

#[test]
fn audit_uses_installed_required_peer_dependencies_as_usage_evidence() {
    let fixture = Fixture::new();
    fixture.write(
        "package.json",
        r#"{
  "dependencies": {
    "chart.js": "^4.5.1",
    "chartjs-node-canvas": "^5.0.0"
  }
}"#,
    );
    fixture.write(
        "node_modules/chartjs-node-canvas/package.json",
        r#"{
  "name": "chartjs-node-canvas",
  "version": "5.0.0",
  "peerDependencies": {"chart.js": "^4.4.8"}
}"#,
    );
    fixture.write(
        "src/report.js",
        "import { ChartJSNodeCanvas } from 'chartjs-node-canvas';\n\
         export const canvas = new ChartJSNodeCanvas({width: 10, height: 10});\n",
    );

    let mut engine = Weavatrix::open(&fixture.root).unwrap();
    let audit = tools::call(
        &mut engine,
        "run_audit",
        json!({"category": "dependencies"}),
    )
    .unwrap();
    let report = &audit["dependency_report"];
    let findings = report["findings"].as_array().unwrap();

    assert!(
        !findings.iter().any(|finding| {
            finding["rule"] == "dependency.unused_declaration" && finding["package"] == "chart.js"
        }),
        "a required peer of an imported package is used dependency evidence: {findings:?}"
    );
    assert!(
        report["peer_obligations"]
            .as_array()
            .is_some_and(|obligations| obligations.iter().any(|obligation| {
                obligation["consumer"] == "chartjs-node-canvas"
                    && obligation["package"] == "chart.js"
                    && obligation["required"] == true
            }))
    );
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
fn audit_treats_the_package_library_as_a_local_dependency() {
    let fixture = Fixture::new();
    fixture.write(
        "Cargo.toml",
        "[package]\nname = \"fixture-core\"\nversion = \"0.1.0\"\n",
    );
    fixture.write("src/lib.rs", "pub fn value() -> usize { 1 }\n");
    fixture.write(
        "src/main.rs",
        "use fixture_core::value;\nfn main() { let _ = value(); }\n",
    );

    let mut engine = Weavatrix::open(&fixture.root).unwrap();
    let audit = tools::call(&mut engine, "run_audit", json!({})).unwrap();
    let findings = audit["dependency_report"]["findings"].as_array().unwrap();

    assert!(
        !findings
            .iter()
            .any(|finding| finding["package"] == "fixture_core"),
        "a Cargo target may import its sibling library: {findings:?}"
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
        "fuzz_only",
    ] {
        assert!(
            packages.contains(expected),
            "fixture must exercise package evidence for {expected}: {packages:?}"
        );
    }
    assert!(
        !packages.contains("scale_harness"),
        "a #[path] module is local source, not package evidence: {packages:?}"
    );

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
