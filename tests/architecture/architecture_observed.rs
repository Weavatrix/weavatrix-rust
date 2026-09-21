use crate::support::GitFixture;
use blazingly_json::{Value, json};
use weavatrix_rust::{Weavatrix, tools};

#[test]
fn observed_architecture_has_no_style_and_is_not_the_starter() {
    let fixture = two_modules();
    let mut engine = Weavatrix::open(&fixture.root).unwrap();
    let report = tools::call(&mut engine, "get_architecture_contract", json!({})).unwrap();
    assert_eq!(report["state"], "NOT_CONFIGURED");
    assert_eq!(report["starter"]["style"], "modular-components");
    assert!(report["observed"].get("style").is_none(), "{report}");
    assert_eq!(report["observed"]["kind"], "observed");
    assert!(
        component_id(&report["observed"], "src").is_some(),
        "{report}"
    );
    assert!(
        component_id(&report["observed"], "lib").is_some(),
        "{report}"
    );
}

#[test]
fn an_import_across_folders_is_a_typed_edge() {
    let fixture = two_modules();
    let mut engine = Weavatrix::open(&fixture.root).unwrap();
    let report = tools::call(&mut engine, "architecture_inventory", json!({})).unwrap();
    assert_eq!(report["status"], "COMPLETE");
    assert!(report.get("style").is_none(), "{report}");
    let lib = component_id(&report, "lib").unwrap();
    let src = component_id(&report, "src").unwrap();
    assert!(
        report["edges"].as_array().unwrap().iter().any(|edge| {
            edge["from"] == lib && edge["to"] == src && edge["relation"] == "imports"
        }),
        "{report}"
    );
}

#[test]
fn declared_id_comes_only_from_the_contract() {
    let fixture = two_modules();
    let mut engine = Weavatrix::open(&fixture.root).unwrap();
    let before = tools::call(&mut engine, "architecture_inventory", json!({})).unwrap();
    assert!(
        before["components"]
            .as_array()
            .unwrap()
            .iter()
            .all(|component| component.get("declared_id").is_none()),
        "{before}"
    );

    fixture.write(
        ".weavatrix/architecture.json",
        r#"{"components":[{"id":"core","paths":["src"]},{"id":"adapters","paths":["lib"]}]}"#,
    );
    let mut engine = Weavatrix::open(&fixture.root).unwrap();
    let after = tools::call(&mut engine, "get_architecture_contract", json!({})).unwrap();
    assert_eq!(after["state"], "CONFIGURED");
    assert_eq!(declared(&after["observed"], "src"), Some("core"));
    assert_eq!(declared(&after["observed"], "lib"), Some("adapters"));
}

#[test]
fn a_manifest_is_a_package_and_a_source_file_is_not() {
    let fixture = two_modules();
    let mut engine = Weavatrix::open(&fixture.root).unwrap();
    let report = tools::call(&mut engine, "architecture_inventory", json!({})).unwrap();
    let packages = report["packages"].as_array().unwrap();
    assert!(
        packages.iter().any(|package| {
            package["ecosystem"] == "npm"
                && package["manifest"] == "package.json"
                && package["package_confirmed"] == true
        }),
        "{report}"
    );
    assert!(
        packages
            .iter()
            .all(|package| package["manifest"] != "src/app.js"),
        "{report}"
    );
}

#[test]
fn nested_directories_retain_cross_boundary_dependencies() {
    let fixture = GitFixture::new();
    fixture.write("src/domain/order.js", "export const order = 1;\n");
    fixture.write(
        "src/infra/db.js",
        "import { order } from '../domain/order.js';\nexport const db = order;\n",
    );
    let mut engine = Weavatrix::open(&fixture.root).unwrap();
    let report = tools::call(&mut engine, "architecture_inventory", json!({})).unwrap();
    let owner = |path| {
        report["components"]
            .as_array()
            .unwrap()
            .iter()
            .find(|component| component["path"] == path)
            .unwrap()["id"]
            .clone()
    };
    assert_ne!(owner("src/domain"), owner("src/infra"));
    assert!(
        report["edges"]
            .as_array()
            .unwrap()
            .iter()
            .any(|edge| edge["from"] == owner("src/infra")
                && edge["to"] == owner("src/domain")
                && edge["relation"] == "imports"),
        "{report}"
    );
}

#[test]
fn component_keys_distinguish_underscores_dashes_and_root() {
    let fixture = GitFixture::new();
    fixture.write("core_api/a.js", "export const a = 1;\n");
    fixture.write(
        "core-api/b.js",
        "import { a } from '../core_api/a.js';\nexport const b = a;\n",
    );
    fixture.write("root/a.js", "export const root = 1;\n");
    fixture.write(
        "main.js",
        "import { root } from './root/a.js';\nexport const main = root;\n",
    );
    let mut engine = Weavatrix::open(&fixture.root).unwrap();
    let report = tools::call(&mut engine, "architecture_inventory", json!({})).unwrap();
    let ids = component_ids(&report);
    assert_eq!(ids.len(), 4, "{report}");
    let unique = ids.iter().collect::<std::collections::BTreeSet<_>>();
    assert_eq!(unique.len(), ids.len());
    assert!(ids.iter().any(|id| id.starts_with("component:root-files:")));
    assert_eq!(
        report["edges"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|edge| edge["relation"] == "imports")
            .count(),
        2
    );
}

#[test]
fn declared_membership_preserves_multiple_matches_in_one_observed_component() {
    let fixture = GitFixture::new();
    fixture.write("src/domain/a.js", "export const a = 1;\n");
    fixture.write("src/domain/b.js", "export const b = 1;\n");
    fixture.write(".weavatrix/architecture.json", r#"{"components":[{"id":"a","paths":["src/domain/a.js"]},{"id":"b","paths":["src/domain/b.js"]}]}"#);
    let mut engine = Weavatrix::open(&fixture.root).unwrap();
    let report = tools::call(&mut engine, "architecture_inventory", json!({})).unwrap();
    let component = report["components"]
        .as_array()
        .unwrap()
        .iter()
        .find(|component| component["path"] == "src/domain")
        .unwrap();
    assert_eq!(component["declared_ids"], json!(["a", "b"]));
    assert!(component.get("declared_id").is_none());
}

#[test]
fn invalid_declaration_does_not_erase_observation() {
    let fixture = two_modules();
    fixture.write(".weavatrix/architecture.json", "invalid json");
    let mut engine = Weavatrix::open(&fixture.root).unwrap();
    let report = tools::call(&mut engine, "architecture_inventory", json!({})).unwrap();
    assert_eq!(report["status"], "INCOMPLETE");
    assert_eq!(report["components"].as_array().unwrap().len(), 2);
    assert!(!report["diagnostics"].as_array().unwrap().is_empty());
}

fn two_modules() -> GitFixture {
    let fixture = GitFixture::new();
    fixture.write("package.json", "{\"name\":\"demo\"}\n");
    fixture.write("src/app.js", "export function app() { return 1; }\n");
    fixture.write(
        "lib/use.js",
        "import { app } from '../src/app.js';\nexport function use() { return app(); }\n",
    );
    fixture
}

fn component_ids(observed: &Value) -> Vec<String> {
    observed["components"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|component| component["id"].as_str().map(str::to_owned))
        .collect()
}

fn component_id<'report>(observed: &'report Value, path: &str) -> Option<&'report str> {
    observed["components"]
        .as_array()?
        .iter()
        .find(|component| component["path"] == path)?["id"]
        .as_str()
}

fn declared<'report>(observed: &'report Value, id: &str) -> Option<&'report str> {
    observed["components"]
        .as_array()?
        .iter()
        .find(|component| component["path"] == id)?
        .get("declared_id")?
        .as_str()
}
