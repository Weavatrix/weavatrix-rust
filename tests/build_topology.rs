mod language_fixture;

use blazingly_json::{Value, json};
use language_fixture::Fixture;
use weavatrix_rust::{Weavatrix, tools};

fn workspace<'report>(report: &'report Value, ecosystem: &str) -> &'report Value {
    report["workspaces"]
        .as_array()
        .unwrap()
        .iter()
        .find(|workspace| workspace["ecosystem"] == ecosystem)
        .unwrap_or_else(|| panic!("no {ecosystem} workspace: {report:?}"))
}

#[test]
fn npm_workspaces_expose_members_targets_and_internal_dependencies() {
    let fixture = Fixture::new();
    fixture.write(
        "package.json",
        r#"{"name": "root", "workspaces": ["packages/*"]}"#,
    );
    fixture.write(
        "packages/api/package.json",
        r#"{"name": "@x/api", "scripts": {"build": "tsc -p ."}, "dependencies": {"@x/lib": "1.0.0", "express": "4.0.0"}}"#,
    );
    fixture.write(
        "packages/lib/package.json",
        r#"{"name": "@x/lib", "scripts": {"test": "jest"}}"#,
    );
    fixture.write("packages/api/src/index.js", "export const api = 1;\n");
    fixture.write("packages/lib/src/index.js", "export const lib = 1;\n");
    let mut engine = Weavatrix::open(&fixture.root).unwrap();

    let report = tools::call(&mut engine, "build_graph", json!({})).unwrap();
    let npm = workspace(&report, "npm");

    assert_eq!(npm["aggregator"], "package.json");
    let members = npm["members"].as_array().unwrap();
    assert_eq!(members.len(), 2, "{report:?}");
    let api = members
        .iter()
        .find(|member| member["name"] == "@x/api")
        .unwrap();
    assert_eq!(api["path"], "packages/api");
    assert!(
        api["targets"]
            .as_array()
            .unwrap()
            .iter()
            .any(|target| target["kind"] == "script" && target["name"] == "build"),
        "scripts are build targets: {api:?}"
    );
    let internal = api["internal_dependencies"].as_array().unwrap();
    assert_eq!(
        internal.len(),
        1,
        "express is not a workspace member: {api:?}"
    );
    assert_eq!(internal[0]["name"], "@x/lib");
    assert_eq!(internal[0]["member"], "packages/lib");
}

#[test]
fn cargo_workspaces_resolve_path_dependencies_and_implicit_targets() {
    let fixture = Fixture::new();
    fixture.write(
        "Cargo.toml",
        "[workspace]\nmembers = [\n    \"crates/*\",\n]\n",
    );
    fixture.write(
        "crates/core/Cargo.toml",
        "[package]\nname = \"core\"\nversion = \"0.1.0\"\n",
    );
    fixture.write("crates/core/src/lib.rs", "pub fn value() -> usize { 1 }\n");
    fixture.write(
        "crates/cli/Cargo.toml",
        "[package]\nname = \"cli\"\nversion = \"0.1.0\"\n\n[dependencies]\ncore = { path = \"../core\" }\nserde = \"1\"\n\n[[bin]]\nname = \"cli-extra\"\npath = \"src/extra.rs\"\n",
    );
    fixture.write("crates/cli/src/main.rs", "fn main() {}\n");
    fixture.write("crates/cli/src/extra.rs", "fn main() {}\n");
    let mut engine = Weavatrix::open(&fixture.root).unwrap();

    let report = tools::call(&mut engine, "build_graph", json!({})).unwrap();
    let cargo = workspace(&report, "cargo");

    assert_eq!(cargo["aggregator"], "Cargo.toml");
    let members = cargo["members"].as_array().unwrap();
    assert_eq!(members.len(), 2, "{report:?}");
    let cli = members
        .iter()
        .find(|member| member["name"] == "cli")
        .unwrap();
    let internal = cli["internal_dependencies"].as_array().unwrap();
    assert_eq!(internal.len(), 1, "serde is external: {cli:?}");
    assert_eq!(internal[0]["member"], "crates/core");
    let targets = cli["targets"].as_array().unwrap();
    assert!(
        targets
            .iter()
            .any(|target| target["kind"] == "bin" && target["name"] == "cli-extra"),
        "explicit [[bin]] target: {targets:?}"
    );
    assert!(
        targets
            .iter()
            .any(|target| target["kind"] == "bin" && target["implicit"] == true),
        "implicit src/main.rs target: {targets:?}"
    );
    let core = members
        .iter()
        .find(|member| member["name"] == "core")
        .unwrap();
    assert!(
        core["targets"]
            .as_array()
            .unwrap()
            .iter()
            .any(|target| target["kind"] == "lib" && target["implicit"] == true),
        "implicit src/lib.rs target: {core:?}"
    );
}

#[test]
fn standalone_manifests_and_runner_configurations_are_reported() {
    let fixture = Fixture::new();
    fixture.write(
        "package.json",
        r#"{"name": "single", "scripts": {"test": "jest"}}"#,
    );
    fixture.write("src/index.js", "export const value = 1;\n");
    fixture.write("jest.config.cjs", "module.exports = {};\n");
    fixture.write("tsconfig.json", "{}\n");
    fixture.write(
        ".github/workflows/ci.yml",
        "name: CI\njobs:\n  build:\n    runs-on: ubuntu-latest\n",
    );
    let mut engine = Weavatrix::open(&fixture.root).unwrap();

    let report = tools::call(&mut engine, "build_graph", json!({})).unwrap();
    let npm = workspace(&report, "npm");
    assert_eq!(npm["aggregator"], "package.json");
    assert_eq!(npm["members"].as_array().unwrap().len(), 1);
    assert_eq!(npm["members"][0]["name"], "single");

    let runners = report["runners"].as_array().unwrap();
    for expected in ["jest", "typescript", "github-actions"] {
        assert!(
            runners.iter().any(|runner| runner["kind"] == expected),
            "runner {expected} must be inventoried: {runners:?}"
        );
    }
    assert_eq!(
        report["model"],
        "manifest and lockfile evidence only; no build tool was executed"
    );
}
