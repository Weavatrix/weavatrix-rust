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
    let ids = component_ids(&report["observed"]);
    assert!(ids.contains(&"src".to_owned()), "{report}");
    assert!(ids.contains(&"lib".to_owned()), "{report}");
}

#[test]
fn an_import_across_folders_is_a_typed_edge() {
    let fixture = two_modules();
    let mut engine = Weavatrix::open(&fixture.root).unwrap();
    let report = tools::call(&mut engine, "architecture_inventory", json!({})).unwrap();
    assert_eq!(report["status"], "COMPLETE");
    assert!(report.get("style").is_none(), "{report}");
    assert!(
        report["edges"].as_array().unwrap().iter().any(|edge| {
            edge["from"] == "lib" && edge["to"] == "src" && edge["relation"] == "imports"
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
            package["ecosystem"] == "npm" && package["manifest"] == "package.json"
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

fn declared<'report>(observed: &'report Value, id: &str) -> Option<&'report str> {
    observed["components"]
        .as_array()?
        .iter()
        .find(|component| component["id"] == id)?
        .get("declared_id")?
        .as_str()
}
