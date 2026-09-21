use crate::language_fixture::Fixture;
use blazingly_json::json;
use weavatrix_rust::{Weavatrix, tools};

fn prepared(source: &str) -> blazingly_json::Value {
    let fixture = Fixture::new();
    fixture.write(
        "Cargo.toml",
        "[package]\nname='demo'\nversion='0.1.0'\n\n[[test]]\nname='policy_guard'\npath='tests/policy.rs'\n",
    );
    fixture.write("src/lib.rs", "pub fn value() {}\n");
    fixture.write("tests/policy.rs", source);
    fixture.write(
        ".weavatrix/architecture.json",
        r#"{"components":[{"id":"source","paths":["src"]}],"dependencyRules":[{"id":"guard","from":["source"],"to":["source"]}]}"#,
    );
    fixture.write(
        ".github/workflows/ci.yml",
        "on: push\njobs:\n  policy:\n    runs-on: ubuntu-latest\n    steps:\n      - run: cargo test --test policy_guard\n",
    );
    let mut engine = Weavatrix::open(&fixture.root).unwrap();
    tools::call(
        &mut engine,
        "prepare_change",
        json!({"files": ["src/lib.rs"]}),
    )
    .unwrap()
}

#[test]
fn checker_linkage_uses_resolved_target_source_not_a_hardcoded_target_name() {
    let report = prepared("#[test]\nfn policy() { assert!(verify_architecture().is_ok()); }\n");
    let binding = &report["ci_protection"]["declared_rule_bindings"][0];
    assert_eq!(binding["linkage"], "STATIC_FAILURE_PATH_CANDIDATE");
    let check = &report["ci_protection"]["candidate_checks"][0];
    assert_eq!(check["selected_targets"].as_array().unwrap().len(), 1);
    assert_eq!(binding["checker"], check["id"], "{report}");
}

#[test]
fn comments_and_ignored_results_do_not_bind_architecture_rules() {
    for source in [
        "// assert!(verify_architecture().is_ok());\n#[test] fn policy() {}\n",
        "#[test]\nfn policy() { let _ = verify_architecture(); }\n",
    ] {
        let report = prepared(source);
        assert_eq!(
            report["ci_protection"]["declared_rule_bindings"][0]["linkage"], "UNBOUND",
            "{report}"
        );
    }
}

#[test]
fn cargo_package_and_manifest_selectors_resolve_workspace_targets() {
    let fixture = Fixture::new();
    fixture.write("Cargo.toml", "[workspace]\nmembers=['crates/*']\n");
    fixture.write(
        "crates/b/Cargo.toml",
        "[package]\nname='beta'\nversion='0.1.0'\n\n[[test]]\nname='policy_guard'\npath='tests/policy.rs'\n",
    );
    fixture.write("crates/b/src/lib.rs", "pub fn value() {}\n");
    fixture.write("crates/b/tests/policy.rs", "#[test] fn policy() {}\n");
    fixture.write(
        ".github/workflows/ci.yml",
        "on: push\njobs:\n  policy:\n    runs-on: ubuntu-latest\n    steps:\n      - run: cargo test -p beta --test policy_guard\n      - run: cargo test --manifest-path crates/b/Cargo.toml --test policy_guard\n",
    );
    let mut engine = Weavatrix::open(&fixture.root).unwrap();

    let report = tools::call(&mut engine, "ci_restrictions", json!({})).unwrap();
    let checks = report["restrictions"].as_array().unwrap();
    assert_eq!(checks.len(), 2, "{report}");
    assert!(checks.iter().all(|check| check["target_resolved"] == true));
    assert_eq!(checks[0]["package"], "beta");
    assert_eq!(checks[1]["manifest_path"], "crates/b/Cargo.toml");
}
