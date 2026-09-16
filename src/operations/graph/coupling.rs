/// Coupling relations for reverse walks. Containment and control flow are not
/// treated as the same kind of dependency: `flows_to` stays off this list.
pub(super) fn coupling_relations() -> std::collections::BTreeSet<String> {
    [
        "calls",
        "imports",
        "inherits",
        "implements",
        "re_exports",
        "references",
        "depends_on_output",
        "calls_workflow",
        "reads_variable",
        "writes_variable",
        "binds_input",
        "uses_model",
        "invokes_tool",
        "queries_knowledge",
    ]
    .into_iter()
    .map(str::to_owned)
    .collect()
}
