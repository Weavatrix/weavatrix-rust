use crate::engine::RepositoryState;
use crate::operations::health;
use blazingly_json::Value;

/// The repository path a node's evidence comes from, if any.
pub(crate) fn node_path(node: &weavatrix_graph::Node) -> Option<&str> {
    node.span
        .as_ref()
        .map(|span| span.file.as_str())
        .or_else(|| (node.kind == weavatrix_graph::NodeKind::File).then_some(node.label.as_str()))
}

/// Whether a node belongs in a production-first answer.
///
/// Every tool whose schema offers `include_classified` or `include_tests` must
/// route through this, otherwise the parameter is advertised and ignored and
/// the answer silently mixes test and generated evidence into production
/// review.
pub(crate) fn node_is_visible(state: &RepositoryState, slot: usize, args: &Value) -> bool {
    let index = weavatrix_graph::NodeIndex::new(u32::try_from(slot).unwrap_or(u32::MAX));
    let Some(node) = state.graph().node_at(index) else {
        return true;
    };
    // A domain node's span is where the evidence was written, not a claim that
    // the domain itself is a production file. Classify it from the symbols or
    // files that expose it, so a `#[cfg(test)]` owner still hides the route.
    if !is_projected_domain(node) && node_path(node).is_some() {
        return evidence_node_is_visible(node, args);
    }
    let mut declared = false;
    for edge in state.graph().incoming_at(index) {
        let Some(source) = state.graph().node(edge.source.as_str()) else {
            continue;
        };
        if node_path(source).is_none() {
            continue;
        }
        declared = true;
        if evidence_node_is_visible(source, args) {
            return true;
        }
    }
    // Repository and package nodes have no declaring file; keep them rather
    // than hide evidence.
    !declared
}

fn is_projected_domain(node: &weavatrix_graph::Node) -> bool {
    node.id.as_str().starts_with("domain:")
}

fn evidence_node_is_visible(node: &weavatrix_graph::Node, args: &Value) -> bool {
    if matches!(
        node.attributes.get("test_only"),
        Some(weavatrix_graph::AttributeValue::Bool(true))
    ) {
        return args.get("include_tests").and_then(Value::as_bool) == Some(true);
    }
    node_path(node).is_none_or(|path| health::path_is_visible(path, args))
}
