use super::paths::path_is_visible;
use super::scc::{file_ids, node_index, strongly_connected_files, strongly_connected_nodes};
use blazingly_json::Value;
use std::collections::{BTreeMap, BTreeSet};
use weavatrix_graph::{AttributeValue, EdgeKind, Graph, NodeIndex, NodeKind};

/// Finds actionable cycles after collapsing symbol-level runtime evidence to
/// declaring files.
pub(in crate::operations) fn runtime_dependency_cycles(
    graph: &Graph,
    args: &Value,
) -> Vec<Vec<String>> {
    let owners = file_owners(graph);
    let visible = |index: NodeIndex| {
        graph
            .node_at(index)
            .is_some_and(|node| path_is_visible(&node.label, args))
    };
    let mut adjacency = BTreeMap::<NodeIndex, BTreeSet<NodeIndex>>::new();
    let mut calls = BTreeMap::<NodeIndex, BTreeSet<NodeIndex>>::new();
    let mut transport = BTreeMap::<NodeIndex, (BTreeSet<NodeIndex>, BTreeSet<NodeIndex>)>::new();
    collect_runtime_edges(
        graph,
        &owners,
        &visible,
        &mut adjacency,
        &mut calls,
        &mut transport,
    );
    for (producers, consumers) in transport.values() {
        for producer in producers {
            for consumer in consumers {
                add_runtime_dependency(&mut adjacency, *producer, *consumer, &visible);
            }
        }
    }
    let mut cycles = strongly_connected_files(graph, &adjacency);
    add_call_cycles(graph, &owners, &calls, &mut cycles);
    cycles.sort_unstable();
    cycles.dedup();
    cycles
}

fn file_owners(graph: &Graph) -> BTreeMap<NodeIndex, NodeIndex> {
    let mut files_by_path = BTreeMap::<String, NodeIndex>::new();
    for (slot, node) in graph.nodes().iter().enumerate() {
        if node.kind == NodeKind::File {
            files_by_path.insert(node.label.clone(), node_index(slot));
        }
    }
    graph
        .nodes()
        .iter()
        .enumerate()
        .filter_map(|(slot, node)| {
            if node.kind == NodeKind::File {
                return Some((node_index(slot), node_index(slot)));
            }
            let owner = node
                .span
                .as_ref()
                .and_then(|span| files_by_path.get(&span.file))
                .copied()?;
            Some((node_index(slot), owner))
        })
        .collect()
}

#[allow(clippy::too_many_arguments)]
fn collect_runtime_edges(
    graph: &Graph,
    owners: &BTreeMap<NodeIndex, NodeIndex>,
    visible: &impl Fn(NodeIndex) -> bool,
    adjacency: &mut BTreeMap<NodeIndex, BTreeSet<NodeIndex>>,
    calls: &mut BTreeMap<NodeIndex, BTreeSet<NodeIndex>>,
    transport: &mut BTreeMap<NodeIndex, (BTreeSet<NodeIndex>, BTreeSet<NodeIndex>)>,
) {
    for edge in graph.edges() {
        let (Some(source), Some(target)) = (
            graph.node_index(edge.source.as_str()),
            graph.node_index(edge.target.as_str()),
        ) else {
            continue;
        };
        let Some(source_file) = owners.get(&source).copied() else {
            continue;
        };
        match edge.kind {
            EdgeKind::Imports
                if runtime_import(edge)
                    && runtime_import_language(graph, source_file)
                    && !rust_module_declaration(graph, source, target, edge) =>
            {
                let Some(target_file) = owners.get(&target).copied() else {
                    continue;
                };
                add_runtime_dependency(adjacency, source_file, target_file, visible);
            }
            EdgeKind::Calls if reliable_call(edge) => {
                let Some(target_file) = owners.get(&target).copied() else {
                    continue;
                };
                if visible(source_file) && visible(target_file) {
                    calls.entry(source).or_default().insert(target);
                    calls.entry(target).or_default();
                }
            }
            EdgeKind::Mounts => {
                let Some(target_file) = owners.get(&target).copied() else {
                    continue;
                };
                add_runtime_dependency(adjacency, source_file, target_file, visible);
            }
            EdgeKind::Publishes if visible(source_file) => {
                transport.entry(target).or_default().0.insert(source_file);
            }
            EdgeKind::Consumes if visible(source_file) => {
                transport.entry(target).or_default().1.insert(source_file);
            }
            _ => {}
        }
    }
}

fn add_call_cycles(
    graph: &Graph,
    owners: &BTreeMap<NodeIndex, NodeIndex>,
    calls: &BTreeMap<NodeIndex, BTreeSet<NodeIndex>>,
    cycles: &mut Vec<Vec<String>>,
) {
    // File cycles need a real cyclic call chain, not unrelated calls in both
    // directions between their owning files.
    for component in strongly_connected_nodes(calls) {
        let files = component
            .into_iter()
            .filter_map(|node| owners.get(&node).copied())
            .collect::<BTreeSet<_>>();
        if files.len() > 1 {
            cycles.push(file_ids(graph, files));
        }
    }
}

fn runtime_import(edge: &weavatrix_graph::Edge) -> bool {
    matches!(
        edge.attributes.get("coupling"),
        Some(AttributeValue::String(coupling)) if coupling == "runtime"
    )
}

fn runtime_import_language(graph: &Graph, source_file: NodeIndex) -> bool {
    graph.node_at(source_file).is_some_and(|node| {
        matches!(
            node.language.as_deref(),
            Some("javascript" | "typescript" | "python" | "go" | "bash" | "swift")
        )
    })
}

fn reliable_call(edge: &weavatrix_graph::Edge) -> bool {
    edge.provenance
        .detail
        .as_deref()
        .is_some_and(|detail| detail == "resolved through an import of the defining module")
}

/// `mod child;` composes a Rust module tree; it does not execute an import.
fn rust_module_declaration(
    graph: &Graph,
    source: NodeIndex,
    target: NodeIndex,
    edge: &weavatrix_graph::Edge,
) -> bool {
    let (Some(source_node), Some(target_node), Some(edge_span)) = (
        graph.node_at(source),
        graph.node_at(target),
        edge.provenance.span.as_ref(),
    ) else {
        return false;
    };
    if source_node.language.as_deref() != Some("rust") || target_node.kind != NodeKind::File {
        return false;
    }
    let target_path = std::path::Path::new(&target_node.label);
    let module_name = if target_path.file_name().is_some_and(|name| name == "mod.rs") {
        target_path
            .parent()
            .and_then(std::path::Path::file_name)
            .and_then(|name| name.to_str())
    } else {
        target_path.file_stem().and_then(|name| name.to_str())
    };
    let Some(module_name) = module_name else {
        return false;
    };
    graph.nodes().iter().any(|node| {
        node.kind == NodeKind::Module
            && node.label == module_name
            && node.span.as_ref().is_some_and(|span| {
                span.file == source_node.label
                    && span.start >= edge_span.start
                    && span.end <= edge_span.end
            })
    })
}

fn add_runtime_dependency(
    adjacency: &mut BTreeMap<NodeIndex, BTreeSet<NodeIndex>>,
    source: NodeIndex,
    target: NodeIndex,
    visible: &impl Fn(NodeIndex) -> bool,
) {
    if source != target && visible(source) && visible(target) {
        adjacency.entry(source).or_default().insert(target);
        adjacency.entry(target).or_default();
    }
}
