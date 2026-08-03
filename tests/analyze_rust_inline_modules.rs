#![cfg(feature = "lang-rust")]

mod language_fixture;

use language_fixture::Fixture;
use weavatrix_rust::{Analyzer, EdgeKind, NodeKind};

#[test]
fn inline_module_super_paths_do_not_escape_the_owning_file() {
    let fixture = Fixture::new();
    fixture.write(
        "src/lib.rs",
        r"
mod application;
mod coordinates;

pub use application::PreparedEdits;
pub use coordinates::LineIndex;
",
    );
    fixture.write("src/application.rs", "pub struct PreparedEdits;\n");
    fixture.write(
        "src/coordinates.rs",
        r"
pub struct LineIndex;
fn local_helper() {}

#[cfg(test)]
mod tests {
    use super::{LineIndex, local_helper};

    const _: Option<super::LineIndex> = None;
    fn direct_call() {
        super::local_helper();
    }
}

",
    );

    let snapshot = Analyzer::default().analyze(&fixture.root).unwrap();
    assert!(snapshot.edges.iter().any(|edge| {
        edge.kind == EdgeKind::ReExports
            && edge.source.as_str() == "file:src/lib.rs"
            && edge.target.as_str() == "file:src/application.rs"
    }));
    assert!(
        snapshot.edges.iter().all(|edge| {
            edge.kind != EdgeKind::Imports || edge.source.as_str() != "file:src/coordinates.rs"
        }),
        "an inline module's parent import is local to its owning file: {:#?}",
        snapshot
            .edges
            .iter()
            .filter(|edge| edge.source.as_str() == "file:src/coordinates.rs")
            .collect::<Vec<_>>()
    );

    let helper = snapshot
        .nodes
        .iter()
        .find(|node| node.kind == NodeKind::Function && node.label == "local_helper")
        .unwrap();
    let direct_call = snapshot
        .nodes
        .iter()
        .find(|node| node.kind == NodeKind::Function && node.label == "direct_call")
        .unwrap();
    assert!(snapshot.edges.iter().any(|edge| {
        edge.kind == EdgeKind::Calls && edge.source == direct_call.id && edge.target == helper.id
    }));
}

#[test]
fn nested_inline_paths_keep_real_cross_file_dependencies() {
    let fixture = Fixture::new();
    fixture.write("src/lib.rs", "mod coordinates;\nmod root_sibling;\n");
    fixture.write("src/root_sibling.rs", "pub struct RootType;\n");
    fixture.write(
        "src/coordinates.rs",
        r"
mod external_child;
pub struct LineIndex;

mod outer {
    mod external_child;
    pub struct OuterType;

    mod inner {
        use self::Local;
        use super::OuterType;
        use super::external_child::Child;
        use super::super::LineIndex;
        use super::super::super::root_sibling::RootType;

        struct Local;
    }
}
",
    );
    fixture.write(
        "src/coordinates/external_child.rs",
        "pub struct WrongChild;\n",
    );
    fixture.write(
        "src/coordinates/outer/external_child.rs",
        "pub struct Child;\n",
    );

    let snapshot = Analyzer::default().analyze(&fixture.root).unwrap();
    let imports = snapshot
        .edges
        .iter()
        .filter(|edge| {
            edge.kind == EdgeKind::Imports && edge.source.as_str() == "file:src/coordinates.rs"
        })
        .collect::<Vec<_>>();
    for target in [
        "file:src/coordinates/external_child.rs",
        "file:src/coordinates/outer/external_child.rs",
        "file:src/root_sibling.rs",
    ] {
        assert!(
            imports.iter().any(|edge| edge.target.as_str() == target),
            "missing legitimate dependency on {target}: {imports:#?}"
        );
    }
    assert!(imports.iter().all(|edge| {
        !matches!(
            edge.target.as_str(),
            "file:src/coordinates.rs" | "file:src/lib.rs"
        )
    }));
    assert!(imports.iter().any(|edge| {
        edge.target.as_str() == "file:src/coordinates/outer/external_child.rs"
            && edge
                .provenance
                .detail
                .as_deref()
                .is_some_and(|detail| detail.contains("self::outer::external_child::Child"))
    }));
}
