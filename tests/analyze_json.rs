use weavatrix_rust::{Analyzer, NodeKind};

mod support;
use support::GitFixture;

#[test]
fn json_configuration_and_lockfiles_are_graph_files() {
    let fixture = GitFixture::new();
    fixture.write("package.json", r#"{"name":"fixture"}"#);
    fixture.write(
        "package-lock.json",
        r#"{"name":"fixture","lockfileVersion":3}"#,
    );
    fixture.write(
        ".weavatrix/architecture.json",
        r#"{"version":1,"layers":[]}"#,
    );
    fixture.write("src/main.js", "export function main() { return 1; }\n");
    fixture.commit("json inventory");

    let snapshot = Analyzer::default().analyze(&fixture.root).unwrap();
    for path in [
        "package.json",
        "package-lock.json",
        ".weavatrix/architecture.json",
    ] {
        assert!(
            snapshot.nodes.iter().any(|node| {
                node.kind == NodeKind::File
                    && node.label == path
                    && node.language.as_deref() == Some("json")
            }),
            "{path} must remain in the repository graph"
        );
    }
}
