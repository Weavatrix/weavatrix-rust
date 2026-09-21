use crate::language_fixture::Fixture;
use blazingly_json::json;
use weavatrix_rust::{Weavatrix, tools};

#[test]
fn repeated_commands_have_distinct_workflow_spans() {
    let fixture = Fixture::new();
    fixture.write("src/lib.rs", "pub fn value() {}\n");
    fixture.write(".github/workflows/ci.yml", "on: push\njobs:\n  test:\n    runs-on: ubuntu-latest\n    steps:\n      - run: cargo test\n      - run: cargo test\n");
    let mut engine = Weavatrix::open(&fixture.root).unwrap();
    let report = tools::call(&mut engine, "ci_restrictions", json!({})).unwrap();
    let spans = report["restrictions"]
        .as_array()
        .unwrap()
        .iter()
        .map(|item| item["source"]["byte_span"]["start"].as_u64().unwrap())
        .collect::<Vec<_>>();
    assert_eq!(spans.len(), 2, "{report}");
    assert!(spans[0] < spans[1]);
}

#[test]
fn repeated_commands_across_reverse_named_jobs_keep_source_order() {
    let fixture = Fixture::new();
    fixture.write("src/lib.rs", "pub fn value() {}\n");
    fixture.write(".github/workflows/ci.yml", "on: push\njobs:\n  z_job:\n    runs-on: ubuntu-latest\n    steps:\n      - run: cargo test\n  a_job:\n    runs-on: ubuntu-latest\n    steps:\n      - run: cargo test\n");
    let mut engine = Weavatrix::open(&fixture.root).unwrap();
    let report = tools::call(&mut engine, "ci_restrictions", json!({})).unwrap();
    let checks = report["restrictions"].as_array().unwrap();
    assert!(
        checks[0]["source"]["byte_span"]["start"].as_u64().unwrap()
            < checks[1]["source"]["byte_span"]["start"].as_u64().unwrap()
    );
    assert_eq!(checks[0]["source"]["job"], "z_job");
    assert_eq!(checks[1]["source"]["job"], "a_job");
}

#[test]
fn root_file_membership_and_composite_caps_are_visible() {
    let fixture = Fixture::new();
    fixture.write(
        "Cargo.toml",
        "[package]\nname = 'sample'\nversion = '0.1.0'\n",
    );
    fixture.write("src/lib.rs", "pub fn value() {}\n");
    let rules = (0..51)
        .map(|id| json!({"id": format!("rule_{id}"), "from": ["root"], "to": ["root"]}))
        .collect::<Vec<_>>();
    fixture.write(
        ".weavatrix/architecture.json",
        &blazingly_json::to_string(&json!({
            "components": [{"id": "root", "paths": ["Cargo.toml"]}],
            "dependencyRules": rules
        }))
        .unwrap(),
    );
    let mut engine = Weavatrix::open(&fixture.root).unwrap();
    let files = std::iter::once("Cargo.toml".to_owned())
        .chain((0..100).map(|index| format!("unknown_{index}.rs")))
        .collect::<Vec<_>>();
    let prepared = tools::call(&mut engine, "prepare_change", json!({"files": files})).unwrap();
    let protection = &prepared["ci_protection"];
    assert_eq!(protection["status"], "INCOMPLETE");
    assert_eq!(protection["changed_total"], 101);
    assert_eq!(protection["declared_rule_bindings_total"], 51);
    assert_eq!(
        protection["declared_rule_bindings"]
            .as_array()
            .unwrap()
            .len(),
        50
    );
    assert!(
        protection["changed"][0]["component"]
            .as_str()
            .unwrap()
            .starts_with("component:root-files:")
    );
    assert_eq!(
        protection["changed"][0]["declared_components"],
        json!(["root"])
    );
}

#[test]
fn ci_binding_uses_changed_files_membership_not_whole_directory() {
    let fixture = Fixture::new();
    fixture.write("src/app.js", "export const app = 1;\n");
    fixture.write("src/domain/a.js", "export const a = 1;\n");
    fixture.write("src/domain/b.js", "export const b = 1;\n");
    fixture.write(
        ".weavatrix/architecture.json",
        r#"{
        "components": [{"id":"a", "paths":["src/domain/a.js"]},
                       {"id":"b", "paths":["src/domain/b.js"]}],
        "dependencyRules":[{"id":"only_b", "from":["b"], "to":["a"]}]
    }"#,
    );
    let mut engine = Weavatrix::open(&fixture.root).unwrap();
    let prepared = tools::call(
        &mut engine,
        "prepare_change",
        json!({"files": ["src/domain/a.js"]}),
    )
    .unwrap();
    let protection = &prepared["ci_protection"];
    assert!(
        protection["changed"][0]["component"]
            .as_str()
            .unwrap()
            .starts_with("component:directory:")
    );
    assert_eq!(
        protection["changed"][0]["declared_components"],
        json!(["a"])
    );
    assert_eq!(protection["declared_rule_bindings_total"], 0);
}

#[test]
fn ci_candidates_select_build_targets_and_tasks() {
    let fixture = Fixture::new();
    fixture.write("Cargo.toml", "[package]\nname = 'demo'\nversion = '0.1.0'\n\n[[test]]\nname = 'architecture'\npath = 'tests/architecture.rs'\n");
    fixture.write("src/lib.rs", "pub fn value() {}\n");
    fixture.write("tests/architecture.rs", "#[test] fn architecture() {}\n");
    fixture.write(
        "package.json",
        r#"{"name":"demo-js","scripts":{"typecheck":"tsc --noEmit"}}"#,
    );
    fixture.write(".github/workflows/ci.yml", "on: push\njobs:\n  checks:\n    runs-on: ubuntu-latest\n    steps:\n      - run: cargo test --test architecture\n      - run: npm run typecheck\n");
    let mut engine = Weavatrix::open(&fixture.root).unwrap();
    let prepared = tools::call(
        &mut engine,
        "prepare_change",
        json!({"files": ["src/lib.rs"]}),
    )
    .unwrap();
    let candidates = prepared["ci_protection"]["candidate_checks"]
        .as_array()
        .unwrap();
    let cargo = candidates
        .iter()
        .find(|item| item["kind"] == "cargo_test")
        .unwrap();
    assert_eq!(cargo["relation"], "TARGET_CHECK_CANDIDATE", "{prepared}");
    assert!(!cargo["selected_targets"].as_array().unwrap().is_empty());
    let npm = candidates
        .iter()
        .find(|item| item["kind"] == "npm_script")
        .unwrap();
    assert_eq!(npm["relation"], "TASK_CHECK_CANDIDATE", "{prepared}");
    assert!(!npm["selected_tasks"].as_array().unwrap().is_empty());
    assert_eq!(
        prepared["ci_protection"]["changed"][0]["build_membership"]["resolution"],
        "EXACT_TARGET_PATH"
    );
}
