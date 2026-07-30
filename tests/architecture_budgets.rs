#[allow(dead_code)]
mod support;

use blazingly_json::{Value, json};
use support::GitFixture;
use weavatrix_rust::{Weavatrix, tools};

#[test]
fn oversized_file_and_function_budgets_block_with_stable_evidence() {
    let fixture = GitFixture::new();
    fixture.write(
        "src/large.rs",
        "pub fn large() {\n\
         let one = 1;\n\
         let two = 2;\n\
         let three = 3;\n\
         let four = 4;\n\
         let _sum = one + two + three + four;\n\
         }\n",
    );
    fixture.write(".weavatrix/architecture.json", &contract(5, 3, 0));

    let mut engine = Weavatrix::open(&fixture.root).unwrap();
    let first = tools::call(&mut engine, "verify_architecture", json!({})).unwrap();
    let second = tools::call(&mut engine, "verify_architecture", json!({})).unwrap();

    assert_eq!(first["state"], "BLOCKED");
    assert_eq!(first["enforceable"], true);
    assert_eq!(fingerprints(&first), fingerprints(&second));
    let file = violation(&first, "budget.maxFileLoc");
    assert_eq!(file["evidence"]["file"], "src/large.rs");
    assert_eq!(file["evidence"]["actual"], 7);
    assert_eq!(file["evidence"]["maximum"], 5);
    let function = violation(&first, "budget.maxFunctionLoc");
    assert_eq!(function["evidence"]["file"], "src/large.rs");
    assert_eq!(function["evidence"]["symbol"], "large");
    assert_eq!(function["evidence"]["actual"], 7);
    assert_eq!(function["evidence"]["maximum"], 3);
}

#[test]
fn runtime_cycle_budget_blocks_with_cycle_evidence() {
    let fixture = GitFixture::new();
    fixture.write(
        "src/a.ts",
        "import { b } from './b';\nexport function a() { return b(); }\n",
    );
    fixture.write(
        "src/b.ts",
        "import { a } from './a';\nexport function b() { return a(); }\n",
    );
    fixture.write(".weavatrix/architecture.json", &contract(20, 10, 0));

    let mut engine = Weavatrix::open(&fixture.root).unwrap();
    let report = tools::call(&mut engine, "verify_architecture", json!({})).unwrap();

    assert_eq!(report["state"], "BLOCKED");
    let cycle = violation(&report, "budget.runtimeCycles");
    assert_eq!(cycle["evidence"]["actual"], 1);
    assert_eq!(cycle["evidence"]["maximum"], 0);
    assert_eq!(
        cycle["evidence"]["cycles"],
        json!([["file:src/a.ts", "file:src/b.ts"]])
    );
}

#[test]
fn compliant_fixture_passes_every_budget() {
    let fixture = GitFixture::new();
    fixture.write("src/lib.rs", "pub fn answer() -> u32 {\n    42\n}\n");
    fixture.write(".weavatrix/architecture.json", &contract(10, 5, 0));

    let mut engine = Weavatrix::open(&fixture.root).unwrap();
    let report = tools::call(&mut engine, "verify_architecture", json!({})).unwrap();

    assert_eq!(report["state"], "PASS");
    assert_eq!(report["enforceable"], true);
    assert_eq!(report["new"], json!([]));
}

#[test]
fn missing_contract_remains_explicitly_not_configured() {
    let fixture = GitFixture::new();
    fixture.write("src/lib.rs", "pub fn answer() -> u32 { 42 }\n");

    let mut engine = Weavatrix::open(&fixture.root).unwrap();
    let report = tools::call(&mut engine, "verify_architecture", json!({})).unwrap();

    assert_eq!(report["state"], "NOT_CONFIGURED");
    assert_eq!(report["enforceable"], false);
    assert_eq!(report["starter"]["budgets"]["maxFileLoc"], 300);
    assert_eq!(report["starter"]["budgets"]["maxFunctionLoc"], 100);
    assert_eq!(report["starter"]["budgets"]["runtimeCycles"], 0);
}

#[test]
fn malformed_budget_is_rejected_instead_of_silently_skipped() {
    let fixture = GitFixture::new();
    fixture.write("src/lib.rs", "pub fn answer() -> u32 { 42 }\n");
    fixture.write(
        ".weavatrix/architecture.json",
        r#"{
          "components": [{"id": "src", "paths": ["src"]}],
          "dependencyRules": [],
          "budgets": {"maxFileLoc": "three hundred"}
        }"#,
    );

    let mut engine = Weavatrix::open(&fixture.root).unwrap();
    let error = tools::call(&mut engine, "verify_architecture", json!({}))
        .expect_err("invalid budgets must fail closed");

    assert!(error.contains("maxFileLoc"));
    assert!(error.contains("non-negative integer"));
}

fn contract(max_file: u64, max_function: u64, runtime_cycles: u64) -> String {
    format!(
        r#"{{
          "components": [{{"id": "src", "paths": ["src"]}}],
          "dependencyRules": [],
          "budgets": {{
            "maxFileLoc": {max_file},
            "maxFunctionLoc": {max_function},
            "runtimeCycles": {runtime_cycles}
          }},
          "ratchet": {{"baseline": {{"fingerprints": []}}}}
        }}"#
    )
}

fn violation<'report>(report: &'report Value, rule: &str) -> &'report Value {
    report["new"]
        .as_array()
        .unwrap()
        .iter()
        .find(|item| item["rule"]["id"] == rule)
        .unwrap_or_else(|| panic!("missing violation for {rule}: {report}"))
}

fn fingerprints(report: &Value) -> Vec<&str> {
    report["new"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|item| item["fingerprint"].as_str())
        .collect()
}
