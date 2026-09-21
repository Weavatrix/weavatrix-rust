use crate::language_fixture::Fixture;
use blazingly_json::{Value, json};
use weavatrix_rust::{Weavatrix, tools};

fn workspace<'report>(report: &'report Value, ecosystem: &str) -> &'report Value {
    report["workspaces"]
        .as_array()
        .unwrap()
        .iter()
        .find(|workspace| workspace["ecosystem"] == ecosystem)
        .unwrap()
}

#[test]
fn build_presentation_caps_preserve_totals_and_incomplete_status() {
    let fixture = Fixture::new();
    let scripts = (0..51)
        .map(|i| (format!("step_{i:03}"), format!("node task_{i}")))
        .collect::<std::collections::BTreeMap<_, _>>();
    fixture.write(
        "package.json",
        &blazingly_json::to_string(&json!({"name":"many", "scripts": scripts})).unwrap(),
    );
    fixture.write("src/index.js", "export const value = 1;\n");
    for i in 0..201 {
        fixture.write(&format!("tsconfig_{i:03}.json"), "{}\n");
    }
    let mut engine = Weavatrix::open(&fixture.root).unwrap();
    let report = tools::call(&mut engine, "build_graph", json!({})).unwrap();
    assert_eq!(report["status"], "INCOMPLETE");
    let member = &workspace(&report, "npm")["members"][0];
    assert_eq!(member["tasks_total"], 51);
    assert_eq!(member["tasks"].as_array().unwrap().len(), 50);
    assert_eq!(member["tasks_truncated"], true);
    assert_eq!(report["runners_total"], 201);
    assert_eq!(report["runners"].as_array().unwrap().len(), 200);
    assert_eq!(report["runners_truncated"], true);
}

#[test]
fn invalid_package_manifest_is_not_a_complete_empty_package() {
    let fixture = Fixture::new();
    fixture.write("package.json", "{ broken json\n");
    fixture.write("src/index.js", "export const value = 1;\n");
    let mut engine = Weavatrix::open(&fixture.root).unwrap();
    let report = tools::call(&mut engine, "build_graph", json!({})).unwrap();
    assert_eq!(report["status"], "INCOMPLETE");
    assert_eq!(
        report["manifest_diagnostics"][0]["reason"],
        "invalid_package_json"
    );
}

#[test]
fn cargo_explicit_paths_features_and_autodiscovery_flags_are_visible() {
    let fixture = Fixture::new();
    fixture.write("Cargo.toml", "[package]\nname = 'flags' # comment\nversion = '0.1.0'\nautolib = false\nautobins = false\nautotests = false\n\n[[test]]\nname = 'custom'\npath = 'checks/unique.rs'\nrequired-features = ['audit']\n");
    fixture.write("src/lib.rs", "pub fn value() {}\n");
    fixture.write("src/main.rs", "fn main() {}\n");
    fixture.write("tests/implicit.rs", "#[test] fn t() {}\n");
    fixture.write("checks/unique.rs", "#[test] fn t() {}\n");
    let mut engine = Weavatrix::open(&fixture.root).unwrap();
    let report = tools::call(&mut engine, "build_graph", json!({})).unwrap();
    let cargo = workspace(&report, "cargo");
    let targets = cargo["members"][0]["targets"].as_array().unwrap();
    assert_eq!(targets.len(), 1, "{report}");
    assert_eq!(targets[0]["path"], "checks/unique.rs");
    assert_eq!(targets[0]["required_features"], json!(["audit"]));
    assert_eq!(targets[0]["applicability"], "CONDITIONAL_FEATURES");
}

#[test]
fn cargo_root_package_and_virtual_workspace_remain_distinct() {
    let fixture = Fixture::new();
    fixture.write("Cargo.toml", "[workspace]\nmembers = ['crates/*']\ndefault-members = ['crates/lib']\n\n[package]\nname = 'root-package'\nversion = '0.1.0'\n");
    fixture.write("src/lib.rs", "pub fn value() {}\n");
    fixture.write(
        "crates/lib/Cargo.toml",
        "[package]\nname = 'leaf'\nversion = '0.1.0'\n",
    );
    fixture.write("crates/lib/src/lib.rs", "pub fn leaf() {}\n");
    let mut engine = Weavatrix::open(&fixture.root).unwrap();
    let report = tools::call(&mut engine, "build_graph", json!({})).unwrap();
    let cargo = workspace(&report, "cargo");
    assert_eq!(cargo["members_total"], 2, "{report}");
    assert!(
        cargo["members"]
            .as_array()
            .unwrap()
            .iter()
            .any(|member| member["name"] == "root-package")
    );
    assert_eq!(cargo["default_members"].as_array().unwrap().len(), 1);
    fixture.write("Cargo.toml", "[workspace]\nmembers = ['crates/*']\n");
    let mut virtual_engine = Weavatrix::open(&fixture.root).unwrap();
    let virtual_report = tools::call(&mut virtual_engine, "build_graph", json!({})).unwrap();
    let virtual_cargo = workspace(&virtual_report, "cargo");
    assert_eq!(virtual_cargo["members_total"], 1, "{virtual_report}");
    assert!(
        virtual_cargo["members"]
            .as_array()
            .unwrap()
            .iter()
            .all(|member| member["name"] != "root-package")
    );
}

#[test]
fn cargo_nested_main_is_a_target_but_support_module_is_not() {
    let fixture = Fixture::new();
    fixture.write(
        "Cargo.toml",
        "[package]\nname = 'layout'\nversion = '0.1.0'\n",
    );
    fixture.write("src/lib.rs", "pub fn value() {}\n");
    fixture.write("tests/first.rs", "#[test] fn first() {}\n");
    fixture.write("tests/second/main.rs", "#[test] fn second() {}\n");
    fixture.write("tests/support/mod.rs", "pub fn helper() {}\n");
    let mut engine = Weavatrix::open(&fixture.root).unwrap();
    let report = tools::call(&mut engine, "build_graph", json!({})).unwrap();
    let targets = workspace(&report, "cargo")["members"][0]["targets"]
        .as_array()
        .unwrap();
    assert!(
        targets
            .iter()
            .any(|target| target["path"] == "tests/first.rs")
    );
    assert!(
        targets
            .iter()
            .any(|target| target["path"] == "tests/second/main.rs")
    );
    assert!(
        !targets
            .iter()
            .any(|target| target["path"] == "tests/support/mod.rs")
    );
}

#[test]
fn npm_workspace_negation_and_case_sensitive_paths_do_not_claim_excluded_members() {
    let fixture = Fixture::new();
    fixture.write(
        "package.json",
        r#"{"workspaces":["packages/*", "!packages/skip"]}"#,
    );
    for directory in ["packages/api", "packages/skip", "Packages/api"] {
        fixture.write(
            &format!("{directory}/package.json"),
            &format!(r#"{{"name":"{directory}"}}"#),
        );
        fixture.write(
            &format!("{directory}/src/index.js"),
            "export const value = 1;\n",
        );
    }
    let mut engine = Weavatrix::open(&fixture.root).unwrap();
    let report = tools::call(&mut engine, "build_graph", json!({})).unwrap();
    let npm = report["workspaces"]
        .as_array()
        .unwrap()
        .iter()
        .find(|workspace| workspace["aggregator"] == "package.json")
        .unwrap();
    assert_eq!(npm["members_total"], 1, "{report}");
    assert_eq!(npm["members"][0]["path"], "packages/api");
}

#[test]
fn configuration_refresh_replaces_one_captured_generation() {
    let fixture = Fixture::new();
    fixture.write(
        "Cargo.toml",
        "[package]\nname = 'first'\nversion = '0.1.0'\n",
    );
    fixture.write("src/lib.rs", "pub fn value() {}\n");
    let mut engine = Weavatrix::open(&fixture.root).unwrap();
    let first = tools::call(&mut engine, "build_graph", json!({})).unwrap();
    let generation = first["input_capture"]["generation"].clone();
    assert_eq!(workspace(&first, "cargo")["members"][0]["name"], "first");

    fixture.write(
        "Cargo.toml",
        "[package]\nname = 'second'\nversion = '0.1.0'\n",
    );
    let captured = tools::call(&mut engine, "build_graph", json!({})).unwrap();
    assert_eq!(workspace(&captured, "cargo")["members"][0]["name"], "first");
    assert!(engine.refresh_if_stale().unwrap());
    let refreshed = tools::call(&mut engine, "build_graph", json!({})).unwrap();
    assert_eq!(
        workspace(&refreshed, "cargo")["members"][0]["name"],
        "second"
    );
    assert_ne!(refreshed["input_capture"]["generation"], generation);
}

#[test]
fn typescript_projects_and_references_are_not_npm_tasks() {
    let fixture = Fixture::new();
    fixture.write("package.json", r#"{"name":"web"}"#);
    fixture.write(
        "tsconfig.json",
        r#"{"include":["src/**/*.ts"],"exclude":["dist"],"references":[{"path":"./tsconfig.lib.json"}]}"#,
    );
    fixture.write(
        "tsconfig.lib.json",
        r#"{"files":["src/lib.ts"],"compilerOptions":{"composite":true}}"#,
    );
    fixture.write("src/lib.ts", "export const value = 1;\n");
    let mut engine = Weavatrix::open(&fixture.root).unwrap();
    let report = tools::call(&mut engine, "build_graph", json!({})).unwrap();
    let member = &workspace(&report, "npm")["members"][0];
    assert_eq!(member["tasks"], json!([]));
    assert_eq!(member["targets_total"], 2, "{report}");
    assert!(
        member["targets"]
            .as_array()
            .unwrap()
            .iter()
            .all(|target| target["kind"] == "ts_project")
    );
    let reference = &member["internal_dependencies"][0];
    assert_eq!(reference["scope"], "ts_project_reference");
    assert!(reference["source_target"].as_str().is_some());
    assert!(reference["target"].as_str().is_some());
}

#[test]
fn go_module_packages_commands_and_platform_variants_are_distinct() {
    let fixture = Fixture::new();
    fixture.write("go.mod", "module example.test/app\n\ngo 1.25\n");
    fixture.write("cmd/server/main.go", "package main\nfunc main() {}\n");
    fixture.write("pkg/math/math.go", "package math\nfunc Add() {}\n");
    fixture.write(
        "pkg/math/math_windows.go",
        "//go:build windows\npackage math\nfunc Platform() {}\n",
    );
    let mut engine = Weavatrix::open(&fixture.root).unwrap();
    let report = tools::call(&mut engine, "build_graph", json!({})).unwrap();
    let member = &workspace(&report, "go")["members"][0];
    let modules = member["modules"].as_array().unwrap();
    assert!(modules.iter().any(|module| module["kind"] == "go_command"));
    let package = modules
        .iter()
        .find(|module| module["kind"] == "go_package")
        .unwrap();
    assert_eq!(package["source_files"].as_array().unwrap().len(), 2);
    assert_eq!(package["applicability"], "ALL_WITH_CONDITIONAL_SOURCES");
    assert_eq!(
        package["source_conditions"]["pkg/math/math_windows.go"],
        json!(["go_build:windows", "platform:windows"])
    );
}

#[test]
fn python_distribution_import_packages_and_entry_points_are_distinct() {
    let fixture = Fixture::new();
    fixture.write(
        "pyproject.toml",
        "[project]\nname = 'acme-dist'\nversion = '1.0'\n\n[project.scripts]\nacme = 'acme.cli:main'\n",
    );
    fixture.write("src/acme/__init__.py", "from .cli import main\n");
    fixture.write("src/acme/cli.py", "def main():\n    return 0\n");
    fixture.write("src/shared/tool.py", "VALUE = 1\n");
    let mut engine = Weavatrix::open(&fixture.root).unwrap();
    let report = tools::call(&mut engine, "build_graph", json!({})).unwrap();
    let member = &workspace(&report, "python")["members"][0];
    assert_eq!(member["kind"], "python_distribution");
    assert_eq!(member["name"], "acme-dist");
    assert_eq!(member["targets"], json!([]));
    assert!(
        member["modules"]
            .as_array()
            .unwrap()
            .iter()
            .any(|module| module["kind"] == "python_import_package")
    );
    assert_eq!(member["tasks"][0]["kind"], "entry_point");
    assert_eq!(member["tasks"][0]["command"], "acme.cli:main");
}
