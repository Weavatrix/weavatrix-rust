use crate::language_fixture::Fixture;
use blazingly_json::{Value, json};
use weavatrix_rust::{Weavatrix, tools};

mod p2;

#[test]
fn local_ci_distinguishes_declarations_invocations_and_effects() {
    let fixture = Fixture::new();
    fixture.write(
        "Cargo.toml",
        "[package]\nname = 'sample'\nversion = '0.1.0'\n",
    );
    fixture.write("src/lib.rs", "pub fn answer() -> u8 { 42 }\n");
    fixture.write("tests/architecture.rs", "#[test] fn works() {}\n");
    fixture.write(
        "package.json",
        "{\"scripts\":{\"typecheck\":\"npx tsc --noEmit\"}}\n",
    );
    fixture.write("scripts/check.sh", "set -e\ncargo fmt --all -- --check\n");
    fixture.write(
        ".github/workflows/ci.yml",
        "on: [push, pull_request]\npermissions:\n  contents: read\njobs:\n  quality:\n    runs-on: ubuntu-24.04\n    steps:\n      - run: |\n          # cargo test --test fake\n          echo cargo fmt --all -- --check\n          cargo test --test architecture_self\n          cargo llvm-cov --fail-under-lines 85 --ignore-filename-regex '(main|error)\\.rs$'\n          if cargo tree --locked --all-features | grep -Eq '^(mcport|notify) '; then exit 1; fi\n      - run: cargo test --test architecture\n        continue-on-error: true\n      - uses: ./.github/actions/local-check\n      - run: bash scripts/check.sh\n      - run: npm run typecheck\n",
    );
    fixture.write(
        ".github/actions/local-check/action.yml",
        "runs:\n  using: composite\n  steps:\n    - shell: bash\n      run: cargo fmt --all -- --check\n",
    );
    let mut engine = Weavatrix::open(&fixture.root).unwrap();
    let report = tools::call(
        &mut engine,
        "ci_restrictions",
        json!({"scenario": {"event": "pull_request"}}),
    )
    .unwrap();
    let restrictions = report["restrictions"].as_array().unwrap();
    let old = find(restrictions, "architecture_self");
    assert_eq!(old["target_resolved"], false);
    assert_eq!(old["applicability"], "APPLIES_LOCALLY");
    assert!(old["source"]["byte_span"]["start"].as_u64().is_some());
    let current = find(restrictions, "architecture");
    assert_eq!(current["target_resolved"], true);
    assert_eq!(current["failure_effect"], "SOFTENED");
    assert!(restrictions.iter().any(|item| {
        item["kind"] == "line_coverage_threshold"
            && item["detail"].as_str().unwrap().contains("exclusions")
    }));
    assert!(restrictions.iter().any(|item| {
        item["kind"] == "rustfmt_check"
            && item["source"]["path"] == ".github/actions/local-check/action.yml"
    }));
    assert!(restrictions.iter().any(|item| {
        item["kind"] == "forbidden_transport_dependency"
            && item["recognition"] == "KNOWN_COUNTEREXAMPLE"
    }));
    assert!(restrictions.iter().any(|item| {
        item["kind"] == "rustfmt_check" && item["source"]["path"] == "scripts/check.sh"
    }));
    assert!(restrictions.iter().any(|item| {
        item["kind"] == "typescript_typecheck"
            && item["source"]["path"] == "package.json"
            && item["failure_effect"] == "UNDETERMINED"
    }));
    assert!(restrictions.iter().all(|item| item["target"] != "fake"));
    assert_eq!(report["remote_enforcement"], "NOT_OBSERVED");
    assert_eq!(report["execution"], "NOT_OBSERVED");
    let id = old["id"].as_str().unwrap();
    let explained = tools::call(&mut engine, "explain_restriction", json!({"id": id})).unwrap();
    assert_eq!(explained["status"], "FOUND");
    let prepared = tools::call(
        &mut engine,
        "prepare_change",
        json!({"files": ["src/lib.rs"]}),
    )
    .unwrap();
    assert_eq!(prepared["ci_protection"]["status"], "INCOMPLETE");
    assert_eq!(
        prepared["ci_protection"]["restrictions_status"],
        "INCOMPLETE"
    );
    assert!(
        !prepared["ci_protection"]["restrictions_unresolved"]
            .as_array()
            .unwrap()
            .is_empty()
    );
}

#[test]
fn yaml_aliases_and_dynamic_conditions_do_not_become_gates() {
    let fixture = Fixture::new();
    fixture.write("src/app.js", "export const app = true;\n");
    fixture.write(
        ".github/workflows/unsafe.yml",
        "on: push\njobs: &jobs\n  test:\n    runs-on: ubuntu-latest\n    steps: *jobs\n",
    );
    fixture.write(
        ".github/workflows/conditional.yml",
        "on: pull_request\njobs:\n  test:\n    if: ${{ github.event.pull_request.draft == false }}\n    runs-on: ubuntu-latest\n    steps:\n      - run: cargo test\n",
    );
    let mut engine = Weavatrix::open(&fixture.root).unwrap();
    let report = tools::call(
        &mut engine,
        "ci_restrictions",
        json!({"scenario": {"event": "pull_request"}}),
    )
    .unwrap();
    assert_eq!(report["status"], "INCOMPLETE");
    assert!(
        report["unresolved"]
            .as_array()
            .unwrap()
            .iter()
            .any(|item| { item.as_str().unwrap().contains("YAML anchors") })
    );
    let restriction = &report["restrictions"][0];
    assert_eq!(restriction["applicability"], "UNDETERMINED");
    assert_eq!(restriction["remote_enforcement"], "NOT_OBSERVED");
}

#[test]
fn pull_request_path_filter_can_leave_a_changed_component_unchecked() {
    let fixture = Fixture::new();
    fixture.write("src/shared.js", "export const shared = true;\n");
    fixture.write(
        ".github/workflows/ci.yml",
        "on:\n  pull_request:\n    branches: [main]\n    paths: [frontend/**]\njobs:\n  test:\n    runs-on: ubuntu-latest\n    steps:\n      - run: cargo test\n",
    );
    let mut engine = Weavatrix::open(&fixture.root).unwrap();
    let skipped = tools::call(
        &mut engine,
        "ci_restrictions",
        json!({"scenario": {"event": "pull_request", "branch": "main", "changed_files": ["src/shared.js"]}}),
    )
    .unwrap();
    assert_eq!(skipped["restrictions"][0]["applicability"], "NOT_TRIGGERED");
    let matching = tools::call(
        &mut engine,
        "ci_restrictions",
        json!({"scenario": {"event": "pull_request", "branch": "main", "changed_files": ["frontend/app.js"]}}),
    )
    .unwrap();
    assert_eq!(
        matching["restrictions"][0]["applicability"],
        "APPLIES_LOCALLY"
    );
    let unknown = tools::call(
        &mut engine,
        "ci_restrictions",
        json!({"scenario": {"event": "pull_request", "branch": "main"}}),
    )
    .unwrap();
    assert_eq!(unknown["restrictions"][0]["applicability"], "UNDETERMINED");
}

fn find<'a>(items: &'a [Value], detail: &str) -> &'a Value {
    items
        .iter()
        .find(|item| {
            item["target"] == detail
                || item["detail"]
                    .as_str()
                    .is_some_and(|text| text.contains(detail))
        })
        .unwrap()
}
