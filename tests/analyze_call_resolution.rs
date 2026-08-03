use weavatrix_rust::{Analyzer, EdgeKind, Snapshot, SourceInput};

#[test]
fn member_calls_do_not_bind_to_repository_wide_name_matches() {
    let snapshot = analyze(&[
        (
            "src/app.js",
            "import { includes } from './imported.js';\nfunction parse() {}\nfunction filter() {}\nexport function run(path, values) {\n  JSON.parse('{}');\n  statSync(path).isFile();\n  const note = `non-ASCII — receiver`;\n  return [note, ...values.map(String)].filter(Boolean).includes('x');\n}\n",
        ),
        ("src/unrelated.js", "export function isFile() {}\n"),
        ("src/imported.js", "export function includes() {}\n"),
    ]);

    for (name, declaring_file) in [
        ("parse", "app.js"),
        ("filter", "app.js"),
        ("isFile", "unrelated.js"),
        ("includes", "imported.js"),
    ] {
        assert!(
            snapshot
                .nodes
                .iter()
                .any(|node| node.label == name && node.id.as_str().contains(declaring_file)),
            "the unrelated declaration must exist for the regression to exercise each scope"
        );
        assert!(
            !snapshot.edges.iter().any(|edge| {
                edge.kind == EdgeKind::Calls
                    && snapshot.nodes.iter().any(|node| {
                        node.id == edge.target
                            && node.label == name
                            && node.id.as_str().contains(declaring_file)
                    })
            }),
            "{name} member call must not bind by its final segment"
        );
    }
}

#[test]
fn local_and_import_scoped_calls_still_resolve() {
    let snapshot = analyze(&[
        ("src/lib.js", "export function imported() { return 1; }\n"),
        (
            "src/app.js",
            "import { imported } from './lib.js';\nfunction local() { return 2; }\nexport function run() { return [...imported(), local()]; }\n",
        ),
    ]);

    for (target, detail) in [
        (
            "imported",
            "resolved through an exact imported-name binding",
        ),
        ("local", "resolved in the referencing file's own scope"),
    ] {
        assert!(
            snapshot.edges.iter().any(|edge| {
                edge.kind == EdgeKind::Calls
                    && edge.provenance.detail.as_deref() == Some(detail)
                    && snapshot
                        .nodes
                        .iter()
                        .any(|node| node.id == edge.target && node.label == target)
            }),
            "{target} must retain its scoped call edge"
        );
    }
}

#[test]
fn a_bare_callback_argument_is_reference_evidence_for_its_import() {
    let snapshot = analyze(&[
        (
            "src/formatter.js",
            "export function getReadableTraffic2Fixed(value) { return String(value); }\n",
        ),
        (
            "src/chart.js",
            "import { getReadableTraffic2Fixed } from './formatter.js';\nexport function configure(register) { register(getReadableTraffic2Fixed); }\n",
        ),
    ]);
    let callback = snapshot
        .nodes
        .iter()
        .find(|node| node.label == "getReadableTraffic2Fixed")
        .expect("callback declaration");

    assert!(snapshot.edges.iter().any(|edge| {
        edge.kind == EdgeKind::References
            && edge.target == callback.id
            && edge
                .provenance
                .span
                .as_ref()
                .is_some_and(|span| span.file == "src/chart.js" && span.start.line == 2)
    }));
}

#[test]
fn free_calls_remain_resolvable_in_go_java_and_python() {
    let go = analyze(&[
        ("main.go", "package main\nfunc main() { LoadUser() }\n"),
        ("service.go", "package main\nfunc LoadUser() {}\n"),
    ]);
    assert_call(&go, "LoadUser");

    let java = analyze(&[(
        "App.java",
        "class App { static void helper() {} static void run() { helper(); } }\n",
    )]);
    assert_call(&java, "helper");

    let python = analyze(&[
        (
            "main.py",
            "from service import load_user\n\ndef bootstrap():\n    return load_user()\n",
        ),
        ("service.py", "def load_user():\n    return {}\n"),
    ]);
    assert_call(&python, "load_user");
}

#[test]
fn locally_imported_reexport_binding_remains_resolvable() {
    let snapshot = analyze(&[
        (
            "src/app.js",
            "import { safeRead } from './barrel.js';\nexport function run() { return safeRead('x'); }\n",
        ),
        (
            "src/barrel.js",
            "import { safeRead } from './util.js';\nexport { safeRead };\n",
        ),
        (
            "src/util.js",
            "export function safeRead(path) { return path; }\n",
        ),
    ]);

    assert_call(&snapshot, "safeRead");
}

#[test]
fn exact_binding_follows_its_own_barrel_despite_an_unrelated_name_collision() {
    let snapshot = analyze(&[
        (
            "src/app.js",
            "import { loadGraph } from './graph-barrel.js';\nimport { computeDuplicates } from './duplicates-barrel.js';\nexport function run() { computeDuplicates(); return loadGraph(); }\n",
        ),
        ("src/graph-barrel.js", "export * from './graph-core.js';\n"),
        (
            "src/graph-core.js",
            "export function loadGraph() { return {}; }\n",
        ),
        (
            "src/duplicates-barrel.js",
            "export * from './duplicates-core.js';\n",
        ),
        (
            "src/duplicates-core.js",
            "function loadGraph() { return {}; }\nexport function computeDuplicates() { return loadGraph(); }\n",
        ),
    ]);

    let app_call = snapshot.edges.iter().find(|edge| {
        edge.kind == EdgeKind::Calls
            && edge
                .provenance
                .span
                .as_ref()
                .is_some_and(|span| span.file == "src/app.js" && span.start.line == 3)
            && snapshot
                .nodes
                .iter()
                .any(|node| node.id == edge.target && node.label == "loadGraph")
    });
    let target = app_call
        .and_then(|edge| snapshot.nodes.iter().find(|node| node.id == edge.target))
        .expect("loadGraph imported through graph-barrel must resolve");
    assert!(
        target.id.as_str().contains("graph-core.js"),
        "the exact import binding must not select the unrelated collision: {}",
        target.id
    );
}

#[test]
fn recursive_and_callable_local_names_beat_unrelated_imported_matches() {
    let snapshot = analyze(&[
        (
            "src/app.js",
            "import { other } from './unrelated.js';\nfunction holder() { const count = 1; return count; }\nfunction count(values) { return values.length; }\nfunction visit(node) { if (node) visit(null); }\nexport function run() { visit({}); return count([]); }\n",
        ),
        (
            "src/unrelated.js",
            "export function other() {}\nexport function count() {}\nexport function visit() {}\n",
        ),
    ]);

    for name in ["count", "visit"] {
        assert!(snapshot.edges.iter().any(|edge| {
            edge.kind == EdgeKind::Calls
                && snapshot.nodes.iter().any(|node| {
                    node.id == edge.target
                        && node.label == name
                        && node.id.as_str().contains("app.js")
                })
        }));
        assert!(!snapshot.edges.iter().any(|edge| {
            edge.kind == EdgeKind::Calls
                && snapshot.nodes.iter().any(|node| {
                    node.id == edge.target
                        && node.label == name
                        && node.id.as_str().contains("unrelated.js")
                })
        }));
    }

    let visit = snapshot
        .nodes
        .iter()
        .find(|node| node.label == "visit" && node.id.as_str().contains("app.js"))
        .unwrap();
    assert!(snapshot.edges.iter().any(|edge| {
        edge.kind == EdgeKind::Calls && edge.source == visit.id && edge.target == visit.id
    }));
}

#[test]
fn aliased_imports_bind_to_the_original_export_not_an_unrelated_local_name() {
    let snapshot = analyze(&[
        (
            "src/app.js",
            "import { realPathAllowed as pathAllowed, realRound as round } from './policy.js';\nexport function run(info) { return round(pathAllowed(info)); }\n",
        ),
        (
            "src/policy.js",
            "export function realPathAllowed(info) { return info; }\nexport function realRound(value) { return value; }\n",
        ),
        (
            "src/unrelated.js",
            "export function pathAllowed() {}\nexport function round() {}\n",
        ),
    ]);

    for (imported, unrelated) in [("realPathAllowed", "pathAllowed"), ("realRound", "round")] {
        assert!(snapshot.edges.iter().any(|edge| {
            edge.kind == EdgeKind::Calls
                && snapshot.nodes.iter().any(|node| {
                    node.id == edge.target
                        && node.label == imported
                        && node.id.as_str().contains("policy.js")
                })
        }));
        assert!(!snapshot.edges.iter().any(|edge| {
            edge.kind == EdgeKind::Calls
                && snapshot.nodes.iter().any(|node| {
                    node.id == edge.target
                        && node.label == unrelated
                        && node.id.as_str().contains("unrelated.js")
                })
        }));
    }
}

fn assert_call(snapshot: &Snapshot, target: &str) {
    assert!(
        snapshot.edges.iter().any(|edge| {
            edge.kind == EdgeKind::Calls
                && snapshot
                    .nodes
                    .iter()
                    .any(|node| node.id == edge.target && node.label == target)
        }),
        "{target} free call must resolve"
    );
}

fn analyze(sources: &[(&str, &str)]) -> Snapshot {
    Analyzer::default()
        .analyze_sources(
            std::env::current_dir().unwrap(),
            "call-resolution-test",
            sources.iter().map(|(path, source)| SourceInput {
                path: (*path).to_owned(),
                bytes: source.as_bytes().to_vec(),
                content_hash: None,
            }),
        )
        .unwrap()
}
