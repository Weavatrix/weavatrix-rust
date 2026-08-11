#![cfg(feature = "lang-rust")]

mod language_fixture;

use language_fixture::Fixture;
use std::collections::BTreeSet;
use weavatrix_rust::{Analyzer, EdgeKind, NodeKind, Snapshot};

#[test]
fn resolves_rust_function_values_passed_as_method_arguments() {
    let fixture = Fixture::new();
    fixture.write(
        "src/lib.rs",
        r"
fn safe_virtual_path(value: i32) -> Option<i32> {
    Some(value)
}

fn search_zip(value: Option<i32>) {
    let _ = value.and_then(safe_virtual_path);
}

fn search_tar(value: i32) {
    let _ = safe_virtual_path(value);
}
",
    );

    let snapshot = Analyzer::default().analyze(&fixture.root).unwrap();
    assert_eq!(
        direct_symbol_dependents(&snapshot, "safe_virtual_path"),
        BTreeSet::from([
            ("src/lib.rs".to_owned(), "search_tar".to_owned()),
            ("src/lib.rs".to_owned(), "search_zip".to_owned()),
        ])
    );
}

#[test]
fn resolves_rust_impl_owners_and_associated_call_qualifiers() {
    let fixture = Fixture::new();
    fixture.write(
        "src/options/types.rs",
        r"
pub struct ArchiveOptions;

impl Default for ArchiveOptions {
    fn default() -> Self {
        Self
    }
}
",
    );
    fixture.write(
        "src/options/mod.rs",
        r"
mod types;
pub use types::ArchiveOptions;

pub struct SearchOptions {
    archives: ArchiveOptions,
}

impl Default for SearchOptions {
    fn default() -> Self {
        Self { archives: ArchiveOptions::default() }
    }
}

impl SearchOptions {
    pub fn with_archives(mut self, archives: ArchiveOptions) -> Self {
        self.archives = archives;
        self
    }
}
",
    );

    let snapshot = Analyzer::default().analyze(&fixture.root).unwrap();
    assert_eq!(
        direct_symbol_dependents(&snapshot, "ArchiveOptions"),
        BTreeSet::from([
            ("src/options/mod.rs".to_owned(), "default".to_owned()),
            ("src/options/mod.rs".to_owned(), "with_archives".to_owned()),
            ("src/options/types.rs".to_owned(), "default".to_owned()),
        ])
    );
}

fn direct_symbol_dependents(snapshot: &Snapshot, target: &str) -> BTreeSet<(String, String)> {
    let target = snapshot
        .nodes
        .iter()
        .find(|node| node.label == target)
        .expect("target symbol");
    snapshot
        .edges
        .iter()
        .filter(|edge| {
            matches!(edge.kind, EdgeKind::Calls | EdgeKind::References) && edge.target == target.id
        })
        .filter_map(|edge| snapshot.nodes.iter().find(|node| node.id == edge.source))
        .filter(|node| matches!(node.kind, NodeKind::Function | NodeKind::Method))
        .filter_map(|node| Some((node.span.as_ref()?.file.clone(), node.label.clone())))
        .collect()
}
