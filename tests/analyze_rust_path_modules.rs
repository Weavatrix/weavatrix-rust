#![cfg(feature = "lang-rust")]

mod language_fixture;

use language_fixture::Fixture;
use weavatrix_rust::{Analyzer, EdgeKind, NodeKind};

#[test]
fn resolves_rust_path_modules_as_local_imports() {
    let fixture = Fixture::new();
    fixture.write(
        "src/lib.rs",
        r#"
use local_alias::helper;

#[path = "actual_module.rs"]
mod local_alias;

pub fn run() {
    helper();
}
"#,
    );
    fixture.write("src/actual_module.rs", "pub fn helper() {}\n");

    let snapshot = Analyzer::default().analyze(&fixture.root).unwrap();
    let node = |kind: NodeKind, label: &str| {
        snapshot
            .nodes
            .iter()
            .find(|node| node.kind == kind && node.label == label)
            .unwrap()
    };
    let source = node(NodeKind::File, "src/lib.rs");
    let target = node(NodeKind::File, "src/actual_module.rs");
    let run = node(NodeKind::Function, "run");
    let helper = node(NodeKind::Function, "helper");
    let edge = |kind, source: &str, target: &str| {
        snapshot.edges.iter().any(|edge| {
            edge.kind == kind && edge.source.as_str() == source && edge.target.as_str() == target
        })
    };

    assert!(edge(
        EdgeKind::Imports,
        source.id.as_str(),
        target.id.as_str()
    ));
    assert!(edge(EdgeKind::Calls, run.id.as_str(), helper.id.as_str()));
    assert!(
        !snapshot
            .nodes
            .iter()
            .any(|node| node.kind == NodeKind::Package && node.label == "local_alias")
    );
}
