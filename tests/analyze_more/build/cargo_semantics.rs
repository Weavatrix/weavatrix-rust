use crate::language_fixture::Fixture;
use blazingly_json::{Value, json};
use weavatrix_rust::{Weavatrix, tools};

fn cargo_workspace(report: &Value) -> &Value {
    report["workspaces"]
        .as_array()
        .unwrap()
        .iter()
        .find(|workspace| workspace["ecosystem"] == "cargo")
        .unwrap_or_else(|| panic!("missing Cargo workspace: {report}"))
}

#[test]
fn cargo_structural_toml_preserves_inheritance_aliases_scopes_and_cfg() {
    let fixture = Fixture::new();
    fixture.write(
        "Cargo.toml",
        r#"[workspace]
members = ["crates/*"]
exclude = ["crates/ignored"]
default-members = ["crates/app"]

[workspace.dependencies]
shared = { package = "core-native", path = "crates/core" }
"#,
    );
    fixture.write(
        "crates/app/Cargo.toml",
        r"[package]
name = 'app#safe'
version = '0.1.0'

[dependencies]
shared = { workspace = true }

[dev-dependencies]
core_alias = { package = 'core-native', path = '../core' }

[target.'cfg(unix)'.build-dependencies]
helper_alias = { package = 'helper-native', path = '../helper' }

[[test]]
name = 'suite#one'
required-features = [
  'acceptance',
]
test = false
harness = false
",
    );
    for (path, name) in [
        ("crates/core", "core-native"),
        ("crates/helper", "helper-native"),
        ("crates/ignored", "ignored"),
    ] {
        fixture.write(
            &format!("{path}/Cargo.toml"),
            &format!("[package]\nname = '{name}'\nversion = '0.1.0'\n"),
        );
        fixture.write(&format!("{path}/src/lib.rs"), "pub fn value() {}\n");
    }
    fixture.write("crates/app/tests/suite#one.rs", "#[test] fn works() {}\n");
    let mut engine = Weavatrix::open(&fixture.root).unwrap();

    let report = tools::call(&mut engine, "build_graph", json!({})).unwrap();
    let workspace = cargo_workspace(&report);
    assert_eq!(
        workspace["members_total"], 3,
        "excluded member leaked: {report}"
    );
    assert_eq!(workspace["default_members"].as_array().unwrap().len(), 1);
    let app = workspace["members"]
        .as_array()
        .unwrap()
        .iter()
        .find(|member| member["name"] == "app#safe")
        .unwrap();
    assert_eq!(app["dependencies"].as_array().unwrap().len(), 3, "{app}");
    assert_eq!(app["internal_dependencies"].as_array().unwrap().len(), 3);

    let dependency = |name: &str| {
        app["dependencies"]
            .as_array()
            .unwrap()
            .iter()
            .find(|dependency| dependency["name"] == name)
            .unwrap()
    };
    let inherited = dependency("shared");
    assert_eq!(inherited["native_name"], "core-native");
    assert_eq!(inherited["workspace_inherited"], true);
    assert_eq!(inherited["resolution"], "LOCAL_PATH");
    let alias = dependency("core_alias");
    assert_eq!(alias["native_name"], "core-native");
    assert_eq!(alias["scope"], "dev-dependencies");
    let conditional = dependency("helper_alias");
    assert_eq!(conditional["scope"], "build-dependencies");
    assert_eq!(conditional["condition"], "cargo_target:cfg(unix)");
    assert_eq!(conditional["condition_ast"]["op"], "all");

    let target = app["targets"]
        .as_array()
        .unwrap()
        .iter()
        .find(|target| target["name"] == "suite#one")
        .unwrap();
    assert_eq!(target["path"], "tests/suite#one.rs");
    assert_eq!(target["required_features"], json!(["acceptance"]));
    assert_eq!(target["options"]["test"], false);
    assert_eq!(target["options"]["harness"], false);
}

#[test]
fn invalid_cargo_toml_is_diagnostic_not_empty_success() {
    let fixture = Fixture::new();
    fixture.write("Cargo.toml", "[package\nname = 'broken'\n");
    let mut engine = Weavatrix::open(&fixture.root).unwrap();

    let report = tools::call(&mut engine, "build_graph", json!({})).unwrap();
    assert_eq!(report["status"], "INCOMPLETE");
    assert!(
        report["manifest_diagnostics"]
            .as_array()
            .unwrap()
            .iter()
            .any(|diagnostic| diagnostic["manifest"] == "Cargo.toml"
                && diagnostic["reason"] == "invalid_cargo_manifest")
    );
}
