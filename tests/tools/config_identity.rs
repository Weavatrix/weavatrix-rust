use crate::language_fixture::Fixture;
use blazingly_json::json;
use weavatrix_rust::{SNAPSHOT_SCHEMA_VERSION, Snapshot, Weavatrix, tools};

#[test]
fn build_graph_identity_changes_when_ci_yaml_changes() {
    let fixture = Fixture::new();
    fixture.write("src/app.js", "export function app() { return 1; }\n");
    fixture.write(".github/workflows/ci.yml", "name: one\n");
    let mut engine = Weavatrix::open(&fixture.root).unwrap();
    let first = tools::call(&mut engine, "build_graph", json!({})).unwrap();
    let digest = first["analysis_identity"]["source_manifest_digest"]
        .as_str()
        .unwrap()
        .to_owned();
    assert!(
        first["config_inputs"]["read"]
            .as_array()
            .unwrap()
            .iter()
            .any(|item| item["path"] == ".github/workflows/ci.yml")
    );

    fixture.write("src/app.js", "export function app() { return 2; }\n");
    let mut engine = Weavatrix::open(&fixture.root).unwrap();
    let same = tools::call(&mut engine, "build_graph", json!({})).unwrap();
    assert_eq!(
        same["analysis_identity"]["source_manifest_digest"],
        digest.as_str()
    );

    fixture.write(".github/workflows/ci.yml", "name: two\n");
    let mut engine = Weavatrix::open(&fixture.root).unwrap();
    let changed = tools::call(&mut engine, "build_graph", json!({})).unwrap();
    assert_ne!(
        changed["analysis_identity"]["source_manifest_digest"],
        digest.as_str()
    );
}

#[test]
fn gitignored_workflow_is_excluded_not_read() {
    let fixture = Fixture::new();
    fixture.write(".gitignore", "secret.yml\n");
    fixture.write(".github/workflows/ci.yml", "name: ci\n");
    fixture.write(".github/workflows/secret.yml", "name: secret\n");
    fixture.write("src/app.js", "export function app() { return 1; }\n");
    let mut engine = Weavatrix::open(&fixture.root).unwrap();
    let report = tools::call(&mut engine, "build_graph", json!({})).unwrap();
    let read = report["config_inputs"]["read"].as_array().unwrap();
    let excluded = report["config_inputs"]["excluded"].as_array().unwrap();
    assert!(
        read.iter()
            .any(|item| item["path"] == ".github/workflows/ci.yml")
    );
    assert!(
        read.iter()
            .all(|item| item["path"] != ".github/workflows/secret.yml")
    );
    assert!(
        excluded.iter().any(|item| {
            item["path"] == ".github/workflows/secret.yml" && item["reason"] == "gitignore"
        }),
        "{report}"
    );
}

#[test]
fn snapshot_schema_stays_version_one_without_identity_fields() {
    assert_eq!(SNAPSHOT_SCHEMA_VERSION, 1);
    let json = r#"{"schema_version":1,"generator":"t","repository":"r","revision":"x","capabilities":[],"nodes":[],"edges":[]}"#;
    let snapshot: Snapshot = blazingly_json::from_str(json).unwrap();
    assert_eq!(snapshot.schema_version, 1);
    assert!(snapshot.diagnostics.is_empty());
}
