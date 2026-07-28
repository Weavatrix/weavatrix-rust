use crate::{Analyzer, RepositoryState, SourceInput};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};
use weavatrix_git::{EntryKind, ObjectKind, Repository};
use weavatrix_graph::{Edge, Graph, Node};

pub(super) fn graph_diff(state: &RepositoryState, args: &Value) -> Result<Value, String> {
    let repository = Repository::open(state.root()).map_err(|error| error.to_string())?;
    let base_ref = super::arg_str(args, "base_ref")?;
    let base = super::history::resolve_revision(&repository, base_ref)?;
    let analyzer = Analyzer::default();
    let baseline = revision_graph(&analyzer, &repository, state, base)?;
    let max = usize::try_from(super::arg_u64(args, "max_results").unwrap_or(100))
        .map_err(|_| "max_results is too large")?;

    if let Ok(head_ref) = super::arg_str(args, "head_ref") {
        let head = super::history::resolve_revision(&repository, head_ref)?;
        let target = revision_graph(&analyzer, &repository, state, head)?;
        let base_text = base.to_string();
        let head_text = head.to_string();
        return Ok(compare(
            &baseline,
            &target,
            &base_text,
            &head_text,
            "immutable_git_revision",
            max,
        ));
    }

    let base_text = base.to_string();
    Ok(compare(
        &baseline,
        state.graph(),
        &base_text,
        "WORKTREE",
        "analyzed_worktree",
        max,
    ))
}

/// Analyzes an immutable revision and returns both its graph and the source
/// text it was built from, so source-level reviews can run on the baseline.
pub(super) fn revision_evidence(
    state: &RepositoryState,
    base_ref: &str,
) -> Result<(Graph, Vec<super::health_runtime::Source>), String> {
    let repository = Repository::open(state.root()).map_err(|error| error.to_string())?;
    let revision = super::history::resolve_revision(&repository, base_ref)?;
    let analyzer = Analyzer::default();
    let sources = revision_sources(&analyzer, &repository, revision)?;
    let text = sources
        .iter()
        .filter_map(|source| {
            let language = crate::language::LanguageRegistry::default()
                .adapter_for_extension(
                    &std::path::Path::new(&source.path)
                        .extension()
                        .and_then(|value| value.to_str())
                        .map(str::to_ascii_lowercase)
                        .unwrap_or_default(),
                )
                .map(|adapter| adapter.language().as_str().to_owned())?;
            let body = String::from_utf8(source.bytes.clone()).ok()?;
            Some((source.path.clone(), language, body))
        })
        .collect::<Vec<_>>();
    let snapshot = analyzer
        .analyze_sources(state.root(), revision.to_string(), sources)
        .map_err(|error| error.to_string())?;
    let graph = Graph::try_from_sorted_parts(snapshot.nodes, snapshot.edges)
        .map_err(|error| error.to_string())?;
    Ok((graph, text))
}

fn revision_sources(
    analyzer: &Analyzer,
    repository: &Repository,
    revision: weavatrix_git::ObjectId,
) -> Result<Vec<SourceInput>, String> {
    let snapshot = repository
        .snapshot(&revision.to_string())
        .map_err(|error| error.to_string())?;
    let mut sources = Vec::new();
    for entry in snapshot.entries {
        if entry.kind != EntryKind::Blob {
            continue;
        }
        let path = std::str::from_utf8(&entry.path)
            .map_err(|_| "Git revision contains a non-UTF-8 source path")?
            .to_owned();
        if !analyzer.supports_path(&path) {
            continue;
        }
        let object = repository
            .object(entry.id)
            .map_err(|error| error.to_string())?;
        if object.kind != ObjectKind::Blob {
            return Err(format!("snapshot entry {path} is not a blob"));
        }
        if u64::try_from(object.data.len()).unwrap_or(u64::MAX) > analyzer.max_file_bytes() {
            continue;
        }
        sources.push(SourceInput {
            path,
            bytes: object.data,
            content_hash: Some(entry.id.to_string()),
        });
    }
    Ok(sources)
}

fn revision_graph(
    analyzer: &Analyzer,
    repository: &Repository,
    state: &RepositoryState,
    revision: weavatrix_git::ObjectId,
) -> Result<Graph, String> {
    let snapshot = repository
        .snapshot(&revision.to_string())
        .map_err(|error| error.to_string())?;
    let mut sources = Vec::new();
    for entry in snapshot.entries {
        if entry.kind != EntryKind::Blob {
            continue;
        }
        let path = std::str::from_utf8(&entry.path)
            .map_err(|_| "Git revision contains a non-UTF-8 source path")?
            .to_owned();
        if !analyzer.supports_path(&path) {
            continue;
        }
        let object = repository
            .object(entry.id)
            .map_err(|error| error.to_string())?;
        if object.kind != ObjectKind::Blob {
            return Err(format!("snapshot entry {path} is not a blob"));
        }
        if u64::try_from(object.data.len()).unwrap_or(u64::MAX) > analyzer.max_file_bytes() {
            continue;
        }
        sources.push(SourceInput {
            path,
            bytes: object.data,
            content_hash: Some(entry.id.to_string()),
        });
    }
    let snapshot = analyzer
        .analyze_sources(state.root(), revision.to_string(), sources)
        .map_err(|error| error.to_string())?;
    Graph::try_from_sorted_parts(snapshot.nodes, snapshot.edges).map_err(|error| error.to_string())
}

fn compare(
    baseline: &Graph,
    target: &Graph,
    base: &str,
    head: &str,
    target_kind: &str,
    max: usize,
) -> Value {
    let base_nodes = node_map(baseline);
    let target_nodes = node_map(target);
    let added_nodes = target_nodes
        .iter()
        .filter(|(id, _)| !base_nodes.contains_key(**id))
        .map(|(_, node)| *node)
        .collect::<Vec<_>>();
    let removed_nodes = base_nodes
        .iter()
        .filter(|(id, _)| !target_nodes.contains_key(**id))
        .map(|(_, node)| *node)
        .collect::<Vec<_>>();
    let changed_nodes = target_nodes
        .iter()
        .filter_map(|(id, node)| {
            base_nodes
                .get(id)
                .filter(|baseline| **baseline != *node)
                .map(|baseline| json!({"before": baseline, "after": node}))
        })
        .collect::<Vec<_>>();
    let base_edges = baseline.edges().iter().collect::<BTreeSet<_>>();
    let target_edges = target.edges().iter().collect::<BTreeSet<_>>();
    let added_edges = target_edges
        .difference(&base_edges)
        .copied()
        .collect::<Vec<_>>();
    let removed_edges = base_edges
        .difference(&target_edges)
        .copied()
        .collect::<Vec<_>>();
    json!({
        "base": base,
        "head": head,
        "target_kind": target_kind,
        "counts": {
            "nodes_added": added_nodes.len(),
            "nodes_removed": removed_nodes.len(),
            "nodes_changed": changed_nodes.len(),
            "edges_added": added_edges.len(),
            "edges_removed": removed_edges.len()
        },
        "nodes": {
            "added": added_nodes.into_iter().take(max).collect::<Vec<_>>(),
            "removed": removed_nodes.into_iter().take(max).collect::<Vec<_>>(),
            "changed": changed_nodes.into_iter().take(max).collect::<Vec<_>>()
        },
        "edges": {
            "added": added_edges.into_iter().take(max).collect::<Vec<&Edge>>(),
            "removed": removed_edges.into_iter().take(max).collect::<Vec<&Edge>>()
        },
        "completeness": "COMPLETE_FOR_SUPPORTED_LANGUAGES",
        "source_mutation": "NONE",
        "git_process": "NONE"
    })
}

fn node_map(graph: &Graph) -> BTreeMap<&str, &Node> {
    graph
        .nodes()
        .iter()
        .map(|node| (node.id.as_str(), node))
        .collect()
}
