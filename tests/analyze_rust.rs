#![cfg(feature = "lang-rust")]

use serde_json::Value;
use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};
use weavatrix_rust::{Analyzer, EdgeKind, NodeKind};

#[test]
fn analyzes_a_rust_repository_without_external_processes() {
    let fixture = Fixture::new();
    fixture.write(".gitignore", "ignored.rs\n");
    fixture.write(
        "src/lib.rs",
        r"
pub struct Worker;
pub fn helper() {}
pub fn run() { helper(); }
",
    );
    fixture.write("ignored.rs", "fn should_not_exist() {}\n");

    let first = Analyzer::default().analyze(&fixture.root).unwrap();
    let second = Analyzer::default().analyze(&fixture.root).unwrap();

    assert_eq!(first.revision, second.revision);
    assert_eq!(first.nodes, second.nodes);
    assert!(
        first
            .nodes
            .iter()
            .any(|node| { node.kind == NodeKind::Struct && node.label == "Worker" })
    );
    assert!(
        !first
            .nodes
            .iter()
            .any(|node| node.label == "should_not_exist")
    );

    let helper = first
        .nodes
        .iter()
        .find(|node| node.kind == NodeKind::Function && node.label == "helper")
        .unwrap();
    let run = first
        .nodes
        .iter()
        .find(|node| node.kind == NodeKind::Function && node.label == "run")
        .unwrap();
    assert!(first.edges.iter().any(|edge| {
        edge.kind == EdgeKind::Calls && edge.source == run.id && edge.target == helper.id
    }));
}

#[test]
fn emits_a_javascript_weavatrix_compatible_graph() {
    let fixture = Fixture::new();
    fixture.write(
        "src/lib.rs",
        r"
pub fn helper() {}
pub fn run() { helper(); }
",
    );

    let json = Analyzer::default()
        .analyze_legacy_json(&fixture.root, false)
        .unwrap();
    let graph: Value = serde_json::from_str(&json).unwrap();

    assert_eq!(graph["schemaVersion"], "weavatrix.rust.legacy.v1");
    assert_eq!(graph["edgeTypesV"], 2);
    assert_eq!(graph["edgeProvenanceV"], 1);
    assert!(graph["nodes"].as_array().is_some_and(|nodes| {
        nodes.iter().any(|node| {
            node["kind"] == "function"
                && node["label"] == "run"
                && node["language"] == "rust"
                && node["source_range"]["start"]["line"].is_u64()
        }) && nodes
            .iter()
            .any(|node| node["kind"] == "file" && node["bytes"].is_u64())
    }));
    assert!(graph["links"].as_array().is_some_and(|links| {
        links.iter().any(|link| {
            link["relation"] == "calls"
                && link["provenance"] == "resolved"
                && link["confidence"] == "high"
                && link["line"].is_u64()
        })
    }));
}

struct Fixture {
    root: PathBuf,
}

impl Fixture {
    fn new() -> Self {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "weavatrix-rust-test-{}-{nonce}",
            std::process::id()
        ));
        fs::create_dir_all(&root).unwrap();
        Self { root }
    }

    fn write(&self, relative: &str, contents: &str) {
        let path = self.root.join(relative);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(path, contents).unwrap();
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}
