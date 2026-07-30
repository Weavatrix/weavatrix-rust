#![cfg(feature = "lang-rust")]

use blazingly_json::Value;
use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};
use weavatrix_rust::{Analyzer, EdgeKind, NodeKind};

static FIXTURE_SEQUENCE: AtomicU64 = AtomicU64::new(0);

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
fn resolves_rust_reexports_and_repository_relative_imports() {
    let fixture = Fixture::new();
    fixture.write(
        "Cargo.toml",
        r#"
[package]
name = "fixture-engine"
version = "0.1.0"
"#,
    );
    fixture.write(
        "src/lib.rs",
        r"
mod facts;

pub use facts::{Fact, private_fact};
",
    );
    fixture.write(
        "src/facts.rs",
        r"
pub struct Fact;
pub(crate) fn private_fact() {}
",
    );
    fixture.write(
        "tests/client.rs",
        "use fixture_engine::Fact;\nfn open() { let _ = Fact; }\n",
    );
    fixture.write("README.md", "Read the [guide](docs/guide.md).\n");
    fixture.write("docs/guide.md", "# Guide\n");

    let snapshot = Analyzer::default().analyze(&fixture.root).unwrap();
    let edge = |kind, source: &str, target: &str| {
        snapshot.edges.iter().any(|item| {
            item.kind == kind && item.source.as_str() == source && item.target.as_str() == target
        })
    };

    assert!(
        edge(EdgeKind::ReExports, "file:src/lib.rs", "file:src/facts.rs"),
        "pub use facts::... resolves to the sibling Rust module"
    );
    assert!(
        edge(EdgeKind::Imports, "file:tests/client.rs", "file:src/lib.rs"),
        "the Cargo package name resolves to the current crate root"
    );
    assert!(
        edge(EdgeKind::Imports, "file:README.md", "file:docs/guide.md"),
        "repository-relative Markdown links are local dependencies"
    );
}

#[test]
fn connects_rust_impl_and_trait_methods_to_owners() {
    let fixture = Fixture::new();
    fixture.write(
        "src/lib.rs",
        r"
mod engine_impl;

pub struct Engine;
pub trait Runner {
    fn run(&self);
}
",
    );
    fixture.write(
        "src/engine_impl.rs",
        r"
use crate::{Engine, Runner};

impl Engine {
    pub fn open() -> Self { Self }
}

impl Runner for Engine {
    fn run(&self) {}
}
",
    );

    let snapshot = Analyzer::default().analyze(&fixture.root).unwrap();
    let edge = |kind, source: &str, target: &str| {
        snapshot.edges.iter().any(|item| {
            item.kind == kind && item.source.as_str() == source && item.target.as_str() == target
        })
    };

    let engine = snapshot
        .nodes
        .iter()
        .find(|node| node.kind == NodeKind::Struct && node.label == "Engine")
        .unwrap();
    let runner = snapshot
        .nodes
        .iter()
        .find(|node| node.kind == NodeKind::Trait && node.label == "Runner")
        .unwrap();
    let open = snapshot
        .nodes
        .iter()
        .find(|node| node.kind == NodeKind::Method && node.label == "open")
        .unwrap();
    let trait_run = snapshot
        .nodes
        .iter()
        .find(|node| {
            node.kind == NodeKind::Method
                && node.label == "run"
                && node
                    .span
                    .as_ref()
                    .is_some_and(|span| span.file == "src/lib.rs")
        })
        .unwrap();
    let impl_run = snapshot
        .nodes
        .iter()
        .find(|node| {
            node.kind == NodeKind::Method
                && node.label == "run"
                && node
                    .span
                    .as_ref()
                    .is_some_and(|span| span.file == "src/engine_impl.rs")
        })
        .unwrap();

    assert!(edge(EdgeKind::Method, engine.id.as_str(), open.id.as_str()));
    assert!(edge(
        EdgeKind::Method,
        engine.id.as_str(),
        impl_run.id.as_str()
    ));
    assert!(edge(
        EdgeKind::Method,
        runner.id.as_str(),
        trait_run.id.as_str()
    ));
    assert!(
        snapshot.edges.iter().all(|item| {
            item.kind != EdgeKind::Method
                || snapshot
                    .nodes
                    .iter()
                    .any(|node| node.id == item.target && node.kind == NodeKind::Method)
        }),
        "ordinary lexical nesting never becomes a method edge"
    );
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
    let graph: Value = blazingly_json::from_str(&json).unwrap();

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
            "weavatrix-rust-test-{}-{nonce}-{}",
            std::process::id(),
            FIXTURE_SEQUENCE.fetch_add(1, Ordering::Relaxed)
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
