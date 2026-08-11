#![cfg(feature = "lang-rust")]

mod language_fixture;

use language_fixture::Fixture;
use std::collections::BTreeSet;
use weavatrix_rust::{Analyzer, EdgeKind, NodeKind};

#[test]
fn resolves_rust_types_used_by_function_signatures() {
    let fixture = Fixture::new();
    fixture.write(
        "src/lib.rs",
        r"
pub struct ArchiveOptions {
    cache_bytes: u64,
    archive_limit: usize,
}

pub fn default() -> ArchiveOptions {
    unimplemented!()
}

pub fn with_archives(options: ArchiveOptions) -> ArchiveOptions {
    options
}
",
    );

    let snapshot = Analyzer::default().analyze(&fixture.root).unwrap();
    let archive_options = snapshot
        .nodes
        .iter()
        .find(|node| node.kind == NodeKind::Struct && node.label == "ArchiveOptions")
        .unwrap();
    let referencing_functions = snapshot
        .edges
        .iter()
        .filter(|edge| edge.kind == EdgeKind::References && edge.target == archive_options.id)
        .filter_map(|edge| snapshot.nodes.iter().find(|node| node.id == edge.source))
        .map(|node| node.label.as_str())
        .collect::<BTreeSet<_>>();

    assert_eq!(
        referencing_functions,
        BTreeSet::from(["default", "with_archives"])
    );
}

#[test]
fn qualified_rust_type_does_not_bind_to_a_local_final_name_collision() {
    let fixture = Fixture::new();
    fixture.write(
        "src/lib.rs",
        r"
pub struct Result;

pub fn local_result() -> Result {
    Result
}

pub fn io_result() -> std::io::Result<()> {
    unimplemented!()
}
",
    );

    let snapshot = Analyzer::default().analyze(&fixture.root).unwrap();
    let local_result = snapshot
        .nodes
        .iter()
        .find(|node| node.kind == NodeKind::Struct && node.label == "Result")
        .unwrap();
    let referencing_functions = snapshot
        .edges
        .iter()
        .filter(|edge| edge.kind == EdgeKind::References && edge.target == local_result.id)
        .filter_map(|edge| snapshot.nodes.iter().find(|node| node.id == edge.source))
        .map(|node| node.label.as_str())
        .collect::<BTreeSet<_>>();

    assert_eq!(referencing_functions, BTreeSet::from(["local_result"]));
}
