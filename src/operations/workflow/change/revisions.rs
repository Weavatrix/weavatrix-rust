use super::texts::FileTexts;
use crate::engine::RepositoryState;
use blazingly_json::{Value, json};
use weavatrix_graph::Graph;

pub(super) fn load_graph(
    state: &RepositoryState,
    revision: Option<&str>,
    required: bool,
    unresolved: &mut Vec<Value>,
) -> Option<Graph> {
    let revision = revision?;
    match snapshot(state, revision) {
        Ok(graph) => Some(graph),
        Err(error) if required => {
            unresolved.push(json!({
                "reason": "baseline_unavailable",
                "revision": revision,
                "note": error
            }));
            None
        }
        Err(_) => None,
    }
}

pub(super) fn load_texts(
    state: &RepositoryState,
    files: &[String],
    base_ref: &str,
    head_rev: &str,
    texts: &mut FileTexts,
) {
    for path in files {
        if let Some(text) = file_at(state, path, Some(base_ref)) {
            texts.base.insert(path.clone(), text);
        }
        let head = if head_rev == "WORKTREE" {
            None
        } else {
            Some(head_rev)
        };
        if let Some(text) = file_at(state, path, head) {
            texts.head.insert(path.clone(), text);
        }
    }
}

#[cfg(feature = "git")]
fn snapshot(state: &RepositoryState, revision: &str) -> Result<Graph, String> {
    let repository =
        weavatrix_git::Repository::open(state.root()).map_err(|error| error.to_string())?;
    let id = crate::operations::history::resolve_revision(&repository, revision)?;
    crate::operations::history::revision::revision_graph(
        &crate::analyzer::Analyzer::default(),
        &repository,
        state,
        id,
    )
}

#[cfg(not(feature = "git"))]
fn snapshot(_state: &RepositoryState, _revision: &str) -> Result<Graph, String> {
    Err("git capability is not compiled".to_owned())
}

fn file_at(state: &RepositoryState, path: &str, revision: Option<&str>) -> Option<String> {
    if let Some(revision) = revision {
        return blob(state, path, revision);
    }
    std::fs::read_to_string(state.root().join(path)).ok()
}

#[cfg(feature = "git")]
fn blob(state: &RepositoryState, path: &str, revision: &str) -> Option<String> {
    let repository = weavatrix_git::Repository::open(state.root()).ok()?;
    let id = crate::operations::history::resolve_revision(&repository, revision).ok()?;
    let snapshot = repository.snapshot(&id.to_string()).ok()?;
    let entry = snapshot
        .entries
        .iter()
        .find(|entry| entry.path == path.as_bytes())?;
    let object = repository.object(entry.id).ok()?;
    String::from_utf8(object.data).ok()
}

#[cfg(not(feature = "git"))]
fn blob(_state: &RepositoryState, _path: &str, _revision: &str) -> Option<String> {
    None
}
