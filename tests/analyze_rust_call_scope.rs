//! A Rust call binds only to a name this file can actually name.
//!
//! `values.push(item)` is `Vec::push`, and `Repository::open(root)` is the
//! `open` of somebody else's type. Binding either by its final segment is how
//! one unrelated helper collects every same-named standard-library call in a
//! repository, which put stdlib method names at the top of the connectivity
//! ranking and the hot-path score.

#![cfg(feature = "lang-rust")]

mod language_fixture;

use language_fixture::Fixture;
use weavatrix_rust::{Analyzer, EdgeKind, NodeKind};

const PIPELINE: &str = r"
pub struct Buffer;

pub fn push(value: u32) -> u32 {
    value
}

pub fn helper() -> u32 {
    2
}

impl Buffer {
    pub fn step(&self) -> u32 {
        1
    }

    pub fn run(&self, values: &mut Vec<u32>, path: &str) -> u32 {
        values.push(9);
        let _ = std::fs::read_to_string(path);
        let _ = Elsewhere::open(path);
        self.step() + helper()
    }
}

pub struct Elsewhere;
";

fn analyze() -> weavatrix_rust::Snapshot {
    let fixture = Fixture::new();
    fixture.write("src/lib.rs", "mod pipeline;\n");
    fixture.write("src/pipeline.rs", PIPELINE);
    Analyzer::default().analyze(&fixture.root).unwrap()
}

fn node(snapshot: &weavatrix_rust::Snapshot, kind: &NodeKind, label: &str) -> String {
    snapshot
        .nodes
        .iter()
        .find(|node| node.kind == *kind && node.label == label)
        .unwrap_or_else(|| panic!("{label} must be a {kind:?} node"))
        .id
        .to_string()
}

fn calls(snapshot: &weavatrix_rust::Snapshot, from: &str, to: &str) -> bool {
    snapshot.edges.iter().any(|edge| {
        edge.kind == EdgeKind::Calls && edge.source.as_str() == from && edge.target.as_str() == to
    })
}

#[test]
fn a_method_call_never_binds_to_a_free_function_of_the_same_name() {
    let snapshot = analyze();
    let run = node(&snapshot, &NodeKind::Method, "run");
    let push = node(&snapshot, &NodeKind::Function, "push");

    assert!(
        !calls(&snapshot, &run, &push),
        "values.push(9) is Vec::push, not this file's free `fn push`"
    );
}

#[test]
fn a_self_method_call_binds_to_the_method_on_the_implementing_type() {
    let snapshot = analyze();
    let run = node(&snapshot, &NodeKind::Method, "run");
    let step = node(&snapshot, &NodeKind::Method, "step");

    assert!(
        calls(&snapshot, &run, &step),
        "inside an impl block `self` is the type being implemented, so this \
         one receiver is proven by position"
    );
    let detail = snapshot
        .edges
        .iter()
        .find(|edge| {
            edge.kind == EdgeKind::Calls
                && edge.source.as_str() == run
                && edge.target.as_str() == step
        })
        .and_then(|edge| edge.provenance.detail.clone());
    assert_eq!(
        detail.as_deref(),
        Some("resolved as a method on the implementing type")
    );
}

#[test]
fn a_bare_call_in_the_files_own_scope_still_binds() {
    let snapshot = analyze();
    let run = node(&snapshot, &NodeKind::Method, "run");
    let helper = node(&snapshot, &NodeKind::Function, "helper");

    assert!(
        calls(&snapshot, &run, &helper),
        "a bare `helper()` names a symbol this file can see"
    );
}

/// `Elsewhere::open(path)` names the `open` of a type, and this engine does
/// not infer which one. The two-segment path still records the type it names,
/// so the coupling survives without an invented call.
#[test]
fn a_type_path_call_records_the_type_and_not_a_guessed_target() {
    let snapshot = analyze();
    let run = node(&snapshot, &NodeKind::Method, "run");
    let elsewhere = node(&snapshot, &NodeKind::Struct, "Elsewhere");

    assert!(
        snapshot.edges.iter().any(|edge| {
            edge.kind == EdgeKind::References
                && edge.source.as_str() == run
                && edge.target.as_str() == elsewhere
        }),
        "the owning segment of the path is still evidence"
    );
    assert!(
        snapshot.edges.iter().all(|edge| {
            edge.kind != EdgeKind::Calls
                || edge.source.as_str() != run
                || !edge.target.as_str().contains("read_to_string")
        }),
        "std::fs::read_to_string is not a symbol of this repository"
    );
}
