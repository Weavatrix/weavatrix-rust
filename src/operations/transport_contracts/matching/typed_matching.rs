use super::{EdgeKind, NodeKind, RepositoryState, Value, json};

pub(super) fn typed_nodes<'state>(
    state: &'state RepositoryState,
    args: &Value,
    transport: &str,
) -> Vec<(usize, &'state weavatrix_graph::Node)> {
    state
        .graph()
        .nodes()
        .iter()
        .enumerate()
        .filter(|(_, node)| node.kind == NodeKind::Endpoint)
        .filter(|(_, node)| typed_identity(&node.label, transport).is_some())
        .filter(|(slot, _)| super::super::node_is_visible(state, *slot, args))
        .collect()
}

pub(super) fn typed_client_relations(transport: &str) -> &'static [EdgeKind] {
    if transport == "grpc" {
        &[EdgeKind::Calls, EdgeKind::Exposes]
    } else {
        &[EdgeKind::Calls]
    }
}

pub(super) fn typed_evidence(
    state: &RepositoryState,
    repository: &str,
    endpoint: &weavatrix_graph::Node,
    relations: &[EdgeKind],
    limit: usize,
) -> Vec<Value> {
    state
        .graph()
        .incoming(&endpoint.id)
        .filter(|edge| relations.contains(&edge.kind))
        .filter_map(|edge| {
            let source = state.graph().node(edge.source.as_str())?;
            let span = edge.provenance.span.as_ref();
            Some(json!({
                "repository": repository,
                "role": match edge.kind {
                    EdgeKind::Exposes => "declares",
                    EdgeKind::Calls => "calls",
                    _ => "references"
                },
                "endpoint_id": endpoint.id.as_str(),
                "source_id": source.id.as_str(),
                "source_label": source.label.as_str(),
                "relation": edge.kind.as_str(),
                "extractor": edge.provenance.extractor.as_str(),
                "confidence": format!("{:?}", edge.provenance.confidence).to_ascii_lowercase(),
                "file": span.map(|span| span.file.as_str()),
                "line": span.map(|span| span.start.line),
                "column": span.map(|span| span.start.column)
            }))
        })
        .take(limit)
        .collect()
}

pub(super) fn typed_identity(label: &str, transport: &str) -> Option<String> {
    match transport {
        "graphql" => {
            let remainder = label.strip_prefix("GRAPHQL ")?;
            let (_, field) = remainder.split_once(' ')?;
            Some(format!("graphql:{field}"))
        }
        "grpc" => {
            let remainder = label.strip_prefix("GRPC ")?;
            let rpc = remainder
                .rsplit_once(" [")
                .map_or(remainder, |(rpc, _)| rpc);
            Some(format!("grpc:{rpc}"))
        }
        _ => None,
    }
}

pub(super) fn typed_key(label: &str, transport: &str) -> String {
    format!(
        "{transport}:{}",
        label.to_ascii_lowercase().replace(' ', ":")
    )
}

pub(super) fn typed_mismatch_kind(transport: &str) -> &'static str {
    if transport == "grpc" {
        "STREAMING_MODE_MISMATCH"
    } else {
        "GRAPHQL_OPERATION_MISMATCH"
    }
}

pub(super) fn typed_diagnostics(
    state: &RepositoryState,
    repository: &str,
    transport: &str,
) -> Vec<String> {
    let prefix = if transport == "grpc" {
        "protobuf."
    } else {
        "graphql."
    };
    state
        .snapshot()
        .diagnostics
        .iter()
        .filter(|diagnostic| diagnostic.code.starts_with(prefix))
        .map(|diagnostic| {
            let location = diagnostic.span.as_ref().map_or_else(
                || repository.to_owned(),
                |span| format!("{repository}:{}:{}", span.file, span.start.line),
            );
            format!("{location}: {}: {}", diagnostic.code, diagnostic.message)
        })
        .collect()
}
