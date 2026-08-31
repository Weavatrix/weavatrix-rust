use crate::analyzer::{Analyzer, SourceInput};
use crate::engine::RepositoryState;
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use weavatrix_git::{EntryKind, ObjectId, ObjectKind, Repository};
use weavatrix_graph::Graph;

pub(in crate::operations) fn resolve_revision(
    repository: &Repository,
    value: &str,
) -> Result<ObjectId, String> {
    if let Some((base, hops)) = value.rsplit_once('~') {
        let hops = hops
            .parse::<usize>()
            .map_err(|_| format!("invalid first-parent revision: {value}"))?;
        let mut id = repository
            .resolve(base)
            .map_err(|error| error.to_string())?;
        for _ in 0..hops {
            id = first_parent(repository, id)?;
        }
        Ok(id)
    } else {
        repository.resolve(value).map_err(|error| error.to_string())
    }
}

pub(in crate::operations) fn first_parent(
    repository: &Repository,
    id: ObjectId,
) -> Result<ObjectId, String> {
    repository
        .commit_metadata(id)
        .map_err(|error| error.to_string())?
        .parents
        .first()
        .copied()
        .ok_or_else(|| format!("commit {id} has no parent"))
}

/// Analyzes an immutable revision and retains its source for baseline reviews.
pub(in crate::operations) fn revision_evidence(
    state: &RepositoryState,
    base_ref: &str,
) -> Result<(Graph, Vec<crate::operations::health::runtime::Source>), String> {
    let repository = Repository::open(state.root()).map_err(|error| error.to_string())?;
    let revision = super::resolve_revision(&repository, base_ref)?;
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

/// Exact supported-source changes between a revision and the worktree.
pub(super) fn worktree_changed_files(
    state: &RepositoryState,
    base_ref: &str,
) -> Result<Vec<String>, String> {
    let repository = Repository::open(state.root()).map_err(|error| error.to_string())?;
    let base = super::resolve_revision(&repository, base_ref)?;
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

pub(super) fn revision_graph(
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
