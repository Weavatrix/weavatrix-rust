use weavatrix_rust::{Analyzer, EdgeKind, SourceInput};

#[test]
fn package_imports_recursive_calls_and_f_strings_retain_call_edges() {
    let sources = [
        (
            "src/core.py",
            "def resolve_target(selector: str) -> str:\n    if selector.startswith('#'):\n        return resolve_target(selector[1:])\n    return selector.strip()\n\ndef resolve_target_path(selector: str) -> str:\n    return f\"/{resolve_target(selector)}\"\n",
        ),
        (
            "src/caller.py",
            "from src.core import resolve_target\n\ndef run(value: str) -> str:\n    return resolve_target(value)\n",
        ),
        (
            "src/toplevel.py",
            "from src.core import resolve_target\n\nDEFAULT_TARGET = resolve_target('#main')\n",
        ),
    ];
    let snapshot = Analyzer::default()
        .analyze_sources(
            std::env::current_dir().unwrap(),
            "python-call-resolution-test",
            sources.iter().map(|(path, source)| SourceInput {
                path: (*path).to_owned(),
                bytes: source.as_bytes().to_vec(),
                content_hash: None,
            }),
        )
        .unwrap();
    let target = snapshot
        .nodes
        .iter()
        .find(|node| {
            node.label == "resolve_target"
                && node
                    .span
                    .as_ref()
                    .is_some_and(|span| span.file == "src/core.py")
        })
        .expect("target declaration");
    let mut call_files = snapshot
        .edges
        .iter()
        .filter(|edge| edge.kind == EdgeKind::Calls && edge.target == target.id)
        .filter_map(|edge| edge.provenance.span.as_ref().map(|span| span.file.as_str()))
        .collect::<Vec<_>>();
    call_files.sort_unstable();

    assert_eq!(
        call_files,
        [
            "src/caller.py",
            "src/core.py",
            "src/core.py",
            "src/toplevel.py"
        ],
        "every exact Python call must resolve to the exported declaration; edges: {:?}",
        snapshot.edges
    );
}
