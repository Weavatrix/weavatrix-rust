use crate::{Analyzer, RepositoryState, SourceInput};
use blazingly_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use weavatrix_git::{EntryKind, ObjectKind, Repository};
use weavatrix_graph::{Edge, Graph, Node};

pub(super) fn graph_diff(state: &RepositoryState, args: &Value) -> Result<Value, String> {
    let repository = Repository::open(state.root()).map_err(|error| error.to_string())?;
    let base_ref = super::arg_str(args, "base_ref")?;
    let base = super::history::resolve_revision(&repository, base_ref)?;
    let analyzer = Analyzer::default();
    let baseline = revision_graph(&analyzer, &repository, state, base)?;
    let max = usize::try_from(super::optional_u64(args, "max_results")?.unwrap_or(100))
        .map_err(|_| "max_results is too large")?;

    if let Some(head_ref) = super::optional_str(args, "head_ref")? {
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

/// Exact supported-source changes between an immutable revision and the
/// analyzed worktree. The HEAD fast path delegates tracked comparisons to the
/// Git index reader and uses the scanner manifest only for added/deleted paths;
/// arbitrary baselines compare blob bytes directly.
pub(super) fn worktree_changed_files(
    state: &RepositoryState,
    base_ref: &str,
) -> Result<Vec<String>, String> {
    let repository = Repository::open(state.root()).map_err(|error| error.to_string())?;
    let base = super::history::resolve_revision(&repository, base_ref)?;
    let analyzer = Analyzer::default();
    let snapshot = repository
        .snapshot(&base.to_string())
        .map_err(|error| error.to_string())?;
    let mut baseline = BTreeMap::new();
    for entry in snapshot.entries {
        if entry.kind != EntryKind::Blob {
            continue;
        }
        let path = std::str::from_utf8(&entry.path)
            .map_err(|_| "Git revision contains a non-UTF-8 source path")?
            .replace('\\', "/");
        if analyzer.supports_path(&path) {
            baseline.insert(path, entry.id);
        }
    }
    let current = state
        .scan_report()
        .files
        .iter()
        .map(|file| (file.relative.replace('\\', "/"), file.absolute.as_path()))
        .collect::<BTreeMap<_, _>>();
    let mut changed = BTreeSet::new();

    if repository.head().map_err(|error| error.to_string())?.target == Some(base) {
        for entry in repository.status().map_err(|error| error.to_string())? {
            let path = std::str::from_utf8(&entry.path)
                .map_err(|_| "Git status contains a non-UTF-8 source path")?
                .replace('\\', "/");
            if analyzer.supports_path(&path) {
                changed.insert(path);
            }
        }
        changed.extend(
            baseline
                .keys()
                .filter(|path| !current.contains_key(*path))
                .cloned(),
        );
        changed.extend(
            current
                .keys()
                .filter(|path| !baseline.contains_key(*path))
                .cloned(),
        );
        return Ok(changed.into_iter().collect());
    }

    for (path, absolute) in &current {
        let Some(id) = baseline.remove(path) else {
            changed.insert(path.clone());
            continue;
        };
        let object = repository.object(id).map_err(|error| error.to_string())?;
        if object.kind != ObjectKind::Blob {
            return Err(format!("snapshot entry {path} is not a blob"));
        }
        let actual = fs::read(absolute).map_err(|error| format!("cannot read {path}: {error}"))?;
        if actual != object.data {
            changed.insert(path.clone());
        }
    }
    changed.extend(baseline.into_keys());
    Ok(changed.into_iter().collect())
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
    let sources = revision_sources(analyzer, repository, revision)?;
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
                .filter(|baseline| nodes_differ(baseline, node))
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
        "status": "COMPLETE",
        "git_evidence": {"present": true},
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

/// Immutable Git snapshots carry blob IDs while the scanner carries SHA-256
/// content fingerprints. Both are valid evidence, but comparing their raw
/// strings would mark every unchanged file node as structurally changed.
fn nodes_differ(baseline: &Node, target: &Node) -> bool {
    if baseline.kind != target.kind
        || baseline.id != target.id
        || baseline.label != target.label
        || baseline.language != target.language
        || baseline.span != target.span
    {
        return true;
    }
    let baseline_len = baseline
        .attributes
        .keys()
        .filter(|name| name.as_str() != "content_hash")
        .count();
    let target_len = target
        .attributes
        .keys()
        .filter(|name| name.as_str() != "content_hash")
        .count();
    baseline_len != target_len
        || baseline.attributes.iter().any(|(name, value)| {
            name.as_str() != "content_hash" && target.attributes.get(name) != Some(value)
        })
}
