//! Script scope regressions: member references and bare locals must never
//! bind to unrelated repository symbols by name alone.

use super::*;

/// The repo-lens regression: `body.join("\n")` bound to an unrelated
/// top-level `join` function in another file, through the repository-wide
/// unique-name fallback that member references must never use.
#[test]
fn a_member_reference_never_binds_to_an_unrelated_repository_symbol() {
    let snapshot = analyze(&[
        (
            "src/repos-format.js",
            "export function join(parts) { return parts; }\n",
        ),
        (
            "src/duplicates.js",
            "export function render(body) { return body.join(\"\\n\"); }\n",
        ),
    ]);
    let decoy = snapshot
        .nodes
        .iter()
        .find(|node| node.label == "join" && node.id.as_str().contains("repos-format.js"))
        .expect("the unrelated join declaration must exist to exercise the fallback");
    let offending = snapshot
        .edges
        .iter()
        .filter(|edge| edge.target == decoy.id && edge.kind != EdgeKind::Contains)
        .map(|edge| {
            format!(
                "{} -[{:?}]-> {} ({:?})",
                edge.source, edge.kind, edge.target, edge.provenance.detail
            )
        })
        .collect::<Vec<_>>();
    assert!(
        offending.is_empty(),
        "body.join must not bind to repos-format.js#join by its final segment: {offending:?}"
    );
}

/// The repo-lens regression: a bare function parameter named `lines` bound to
/// a same-named symbol in an unrelated file. Script modules share no global
/// namespace, so an unimported bare name has no cross-file evidence.
#[test]
fn a_bare_local_name_never_binds_across_script_files() {
    let snapshot = analyze(&[
        ("src/metrics.js", "export const lines = [1, 2];\n"),
        (
            "src/duplicates.js",
            "export function classify(file, lines) { return record(file, lines); }\nfunction record(a, b) { return [a, b]; }\n",
        ),
    ]);
    let decoy = snapshot
        .nodes
        .iter()
        .find(|node| node.label == "lines" && node.id.as_str().contains("metrics.js"))
        .expect("the unrelated lines declaration must exist to exercise the fallback");
    assert!(
        !snapshot.edges.iter().any(|edge| {
            edge.target == decoy.id
                && edge
                    .provenance
                    .span
                    .as_ref()
                    .is_some_and(|span| span.file == "src/duplicates.js")
        }),
        "the lines parameter must not bind to metrics.js#lines"
    );
}

/// A `const` inside a function is lexical scope, not a repository symbol:
/// keeping it would offer every same-named local as a resolution target.
#[test]
fn script_function_locals_are_not_graph_symbols() {
    let snapshot = analyze(&[(
        "src/report.js",
        "export function render(parts) {\n  const lines = parts.slice();\n  return lines;\n}\nexport const banner = \"kept\";\n",
    )]);
    assert!(
        !snapshot
            .nodes
            .iter()
            .any(|node| node.label == "lines" && node.id.as_str().contains("report.js")),
        "a function-local const must not become a symbol node"
    );
    assert!(
        snapshot
            .nodes
            .iter()
            .any(|node| node.label == "banner" && node.id.as_str().contains("report.js")),
        "a module-level const stays a symbol node"
    );
}
