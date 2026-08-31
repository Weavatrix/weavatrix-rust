//! Local HTML/CSS resources are file references, never npm dependencies, and
//! deep relative script imports resolve the way Node resolves them.

mod tool_fixture;

use blazingly_json::json;
use tool_fixture::Fixture;
use weavatrix_rust::{Weavatrix, tools};

/// The repo-lens regression: `js/api.js`, `assets/logo.svg` and
/// `./features/coverage/log-panel.css` were reported as missing npm packages.
#[test]
fn local_page_resources_are_not_missing_dependencies() {
    let fixture = Fixture::new();
    fixture.write(
        "index.html",
        "<html><head>\n<link rel=\"stylesheet\" href=\"./features/coverage/log-panel.css\">\n</head><body>\n<img src=\"assets/logo.svg\">\n<script src=\"js/api.js\"></script>\n</body></html>\n",
    );
    fixture.write("js/api.js", "export function api() { return 1; }\n");
    fixture.write(
        "features/coverage/log-panel.css",
        ".panel { color: red; }\n",
    );
    let mut engine = Weavatrix::open(&fixture.root).unwrap();

    let audit = tools::call(
        &mut engine,
        "run_audit",
        json!({"category": "dependencies"}),
    )
    .unwrap();
    let findings = audit["dependency_report"]["findings"].as_array().unwrap();
    assert!(
        !findings
            .iter()
            .any(|finding| finding["rule"] == "dependency.missing_declaration"),
        "local page resources must not be missing dependencies: {findings:?}"
    );
    assert!(
        !engine.state().graph().nodes().iter().any(|node| node
            .id
            .as_str()
            .starts_with("package:html:")
            || node.id.as_str().starts_with("package:css:")),
        "no markup or style specifier may become a package node"
    );
    let page_edges = engine
        .state()
        .snapshot()
        .edges
        .iter()
        .filter(|edge| edge.source.as_str() == "file:index.html")
        .map(|edge| format!("{} ({:?})", edge.target, edge.provenance.detail))
        .collect::<Vec<_>>();
    for target in ["file:js/api.js", "file:features/coverage/log-panel.css"] {
        assert!(
            engine
                .state()
                .snapshot()
                .edges
                .iter()
                .any(|edge| edge.source.as_str() == "file:index.html"
                    && edge.target.as_str() == target),
            "index.html must import {target}; page edges: {page_edges:?}; diagnostics: {:?}; files: {:?}",
            engine.state().snapshot().diagnostics,
            engine
                .state()
                .graph()
                .nodes()
                .iter()
                .filter(|node| node.kind == weavatrix_rust::NodeKind::File)
                .map(|node| node.label.clone())
                .collect::<Vec<_>>()
        );
    }
}

/// A page reference to a missing script is a resolver-visible gap, while a
/// missing image is an asset reference the graph never indexes.
#[test]
fn a_missing_page_script_is_a_gap_and_a_missing_image_is_not() {
    let fixture = Fixture::new();
    fixture.write(
        "index.html",
        "<script src=\"js/gone.js\"></script>\n<img src=\"assets/gone.svg\">\n",
    );
    let engine = Weavatrix::open(&fixture.root).unwrap();
    let diagnostics = &engine.state().snapshot().diagnostics;
    assert!(
        diagnostics
            .iter()
            .any(|item| item.code == "import.unresolved" && item.message.contains("js/gone.js")),
        "a missing local script must surface as import.unresolved: {diagnostics:?}"
    );
    assert!(
        !diagnostics
            .iter()
            .any(|item| item.message.contains("gone.svg")),
        "a non-indexed asset reference proves nothing and stays silent: {diagnostics:?}"
    );
}

/// The repo-lens apimap regression shape: a deep `../` require of a file that
/// exists must resolve without an unresolved-import diagnostic.
#[test]
fn a_deep_relative_require_of_an_existing_file_resolves() {
    let fixture = Fixture::new();
    fixture.write(
        "test/apimap.test.js",
        "const { scan } = require(\"../main/repos/scan/apimap.js\");\nscan();\n",
    );
    fixture.write("main/repos/scan/apimap.js", "exports.scan = () => 1;\n");
    let engine = Weavatrix::open(&fixture.root).unwrap();
    let diagnostics = &engine.state().snapshot().diagnostics;
    assert!(
        !diagnostics
            .iter()
            .any(|item| item.code == "import.unresolved"),
        "an existing deep relative import must resolve: {diagnostics:?}"
    );
    assert!(
        engine
            .state()
            .snapshot()
            .edges
            .iter()
            .any(|edge| edge.source.as_str() == "file:test/apimap.test.js"
                && edge.target.as_str() == "file:main/repos/scan/apimap.js"),
        "the import edge must land on the existing file"
    );
}

/// Node loads modules through the case-insensitive filesystems these projects
/// run on, so a unique case-insensitive match is how the program actually
/// executes.
#[test]
fn a_case_insensitive_script_import_resolves_like_node() {
    let fixture = Fixture::new();
    fixture.write("main/apiMap.js", "exports.scan = () => 1;\n");
    fixture.write(
        "main/caller.js",
        "const { scan } = require(\"./apimap.js\");\nscan();\n",
    );
    let engine = Weavatrix::open(&fixture.root).unwrap();
    let diagnostics = &engine.state().snapshot().diagnostics;
    assert!(
        !diagnostics
            .iter()
            .any(|item| item.code == "import.unresolved"),
        "a unique case-insensitive match must resolve: {diagnostics:?}"
    );
    assert!(
        engine
            .state()
            .snapshot()
            .edges
            .iter()
            .any(|edge| edge.source.as_str() == "file:main/caller.js"
                && edge.target.as_str() == "file:main/apiMap.js"),
        "the import edge must land on the differently-cased file"
    );
}
