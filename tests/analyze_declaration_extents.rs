mod language_fixture;

use language_fixture::Fixture;
use weavatrix_rust::Analyzer;

#[test]
fn graph_symbols_expose_the_full_declaration_extent_for_tokenized_languages() {
    let fixture = Fixture::new();
    fixture.write(
        "src/Button.tsx",
        "export function Button() {\n  return <button>Save</button>;\n}\n",
    );
    fixture.write(
        "src/save.js",
        "export function save() {\n  return true;\n}\n",
    );
    fixture.write(
        "service/save.go",
        "package service\n\nfunc Save() {\n\tprintln(\"save\")\n}\n",
    );

    let snapshot = Analyzer::default().analyze(&fixture.root).unwrap();
    for (path, label, start, end) in [
        ("src/Button.tsx", "Button", 1, 3),
        ("src/save.js", "save", 1, 3),
        ("service/save.go", "Save", 3, 5),
    ] {
        let node = snapshot
            .nodes
            .iter()
            .find(|node| {
                node.label == label
                    && node
                        .span
                        .as_ref()
                        .is_some_and(|span| span.file.as_str() == path)
            })
            .unwrap_or_else(|| panic!("missing {label} in {path}"));
        let span = node.span.as_ref().unwrap();
        assert_eq!(
            (span.start.line, span.end.line),
            (start, end),
            "{path} must expose its declaration body, not only its name"
        );
    }
}
