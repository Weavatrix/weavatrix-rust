use crate::RepositoryState;
#[cfg(feature = "git")]
use crate::tools::{arg_str, arg_u64};
use blazingly_json::Value;
#[cfg(feature = "git")]
use blazingly_json::json;
#[cfg(feature = "git")]
use std::time::{SystemTime, UNIX_EPOCH};

#[cfg(feature = "git")]
use weavatrix_git::RepositorySet;
#[cfg(feature = "git")]
use weavatrix_git::{HistoryOptions, ObjectId, Repository};

#[cfg(feature = "git")]
pub fn history(state: &RepositoryState, args: &Value) -> Result<Value, String> {
    let repository = Repository::open(state.root()).map_err(|error| error.to_string())?;
    let start = resolve_revision(&repository, arg_str(args, "revision").unwrap_or("HEAD"))?;
    let max = usize::try_from(arg_u64(args, "max_commits").unwrap_or(100))
        .map_err(|_| "max_commits is too large")?;
    let months = arg_u64(args, "months").unwrap_or(6);
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|error| error.to_string())?
        .as_secs();
    let window = months.saturating_mul(2_629_746);
    let since = i64::try_from(now.saturating_sub(window)).unwrap_or(i64::MIN);
    let records = repository
        .history(
            start,
            HistoryOptions {
                max_commits: max,
                since: Some(since),
                first_parent: args
                    .get("first_parent")
                    .and_then(Value::as_bool)
                    .unwrap_or(false),
                ..HistoryOptions::default()
            },
        )
        .map_err(|error| error.to_string())?;
    let analytics = super::history_analytics::analyze(state, &repository, &records, args)?;
    Ok(json!({
        "revision": start.to_string(),
        "months": months,
        "analytics": analytics
    }))
}

#[cfg(feature = "git")]
pub fn cross_repo(_state: &RepositoryState, args: &Value) -> Result<Value, String> {
    let inputs = args
        .get("repositories")
        .and_then(Value::as_array)
        .ok_or_else(|| "repositories must be an array".to_owned())?
        .iter()
        .enumerate()
        .map(|(index, item)| {
            let path = item
                .get("path")
                .and_then(Value::as_str)
                .ok_or_else(|| "repository.path must be a string".to_owned())?;
            let name = item
                .get("name")
                .and_then(Value::as_str)
                .map_or_else(|| format!("repository-{index}"), str::to_owned);
            Ok((name, path.to_owned()))
        })
        .collect::<Result<Vec<_>, String>>()?;
    if inputs.is_empty() {
        return Err("repositories must not be empty".to_owned());
    }
    let set = RepositorySet::open_parallel(inputs).map_err(|error| error.to_string())?;
    let options = HistoryOptions {
        max_commits: usize::try_from(arg_u64(args, "max_commits").unwrap_or(1_000))
            .map_err(|_| "max_commits is too large")?,
        first_parent: args
            .get("first_parent")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        ..HistoryOptions::default()
    };
    let revision = arg_str(args, "revision").unwrap_or("HEAD");
    match arg_str(args, "action").unwrap_or("histories") {
        "histories" => {
            let histories = set
                .histories_from_parallel(revision, options)
                .map_err(|error| error.to_string())?;
            Ok(json!({
                "action": "histories",
                "revision": revision,
                "repositories": histories.into_iter().map(|history| json!({
                    "name": set.name(history.repository),
                    "head": history.head.to_string(),
                    "commits": history.commits.into_iter()
                        .map(|id| id.to_string()).collect::<Vec<_>>()
                })).collect::<Vec<_>>()
            }))
        }
        "shared_commits" => {
            let shared = set
                .shared_commits_from(revision, options)
                .map_err(|error| error.to_string())?;
            Ok(json!({
                "action": "shared_commits",
                "revision": revision,
                "commits": shared.into_iter().map(|commit| json!({
                    "id": commit.id.to_string(),
                    "repositories": commit.repositories.into_iter()
                        .filter_map(|id| set.name(id)).collect::<Vec<_>>()
                })).collect::<Vec<_>>()
            }))
        }
        "diff" => {
            let left = arg_str(args, "left")?;
            let right = arg_str(args, "right")?;
            let left_id = set
                .id(left)
                .ok_or_else(|| format!("unknown repository: {left}"))?;
            let right_id = set
                .id(right)
                .ok_or_else(|| format!("unknown repository: {right}"))?;
            let base = arg_str(args, "base_ref").unwrap_or(revision);
            let head = arg_str(args, "head_ref").unwrap_or(revision);
            let changes = set
                .diff_commits(left_id, base, right_id, head)
                .map_err(|error| error.to_string())?;
            Ok(json!({
                "action": "diff",
                "left": {"repository": left, "revision": base},
                "right": {"repository": right, "revision": head},
                "changes": changes.into_iter().map(change_json).collect::<Vec<_>>()
            }))
        }
        action => Err(format!(
            "unknown cross_repo_git action {action:?}; expected histories, shared_commits, or diff"
        )),
    }
}

#[cfg(feature = "git")]
fn change_json(change: weavatrix_git::TreeChange) -> Value {
    json!({
        "path": String::from_utf8_lossy(&change.path),
        "kind": format!("{:?}", change.kind).to_ascii_lowercase(),
        "old": change.old.map(|entry| entry.id.to_string()),
        "new": change.new.map(|entry| entry.id.to_string())
    })
}

#[cfg(not(feature = "git"))]
pub fn cross_repo(_state: &RepositoryState, _args: &Value) -> Result<Value, String> {
    Err("git capability is not compiled".to_owned())
}

#[cfg(not(feature = "git"))]
pub fn history(_state: &RepositoryState, _args: &Value) -> Result<Value, String> {
    Err("git capability is not compiled".to_owned())
}

#[cfg(feature = "git")]
pub fn changes(state: &RepositoryState, args: &Value) -> Result<Value, String> {
    let repository = Repository::open(state.root()).map_err(|error| error.to_string())?;
    let head = resolve_revision(&repository, arg_str(args, "head_ref").unwrap_or("HEAD"))?;
    let base = if let Ok(value) = arg_str(args, "base_ref").or_else(|_| arg_str(args, "base")) {
        resolve_revision(&repository, value)?
    } else {
        first_parent(&repository, head)?
    };
    let changes = repository
        .diff_commits(base, head)
        .map_err(|error| error.to_string())?;
    Ok(json!({
        "base": base.to_string(),
        "head": head.to_string(),
        "changes": changes.into_iter().map(|change| json!({
            "path": String::from_utf8_lossy(&change.path),
            "kind": format!("{:?}", change.kind).to_ascii_lowercase(),
            "old": change.old.map(|entry| entry.id.to_string()),
            "new": change.new.map(|entry| entry.id.to_string())
        })).collect::<Vec<_>>()
    }))
}

#[cfg(not(feature = "git"))]
pub fn changes(_state: &RepositoryState, _args: &Value) -> Result<Value, String> {
    Err("git capability is not compiled".to_owned())
}

#[cfg(feature = "git")]
pub fn graph_diff(state: &RepositoryState, args: &Value) -> Result<Value, String> {
    super::history_diff::graph_diff(state, args)
}

#[cfg(not(feature = "git"))]
pub fn graph_diff(_state: &RepositoryState, _args: &Value) -> Result<Value, String> {
    Err("git capability is not compiled".to_owned())
}

#[cfg(feature = "git")]
pub(super) fn resolve_revision(repository: &Repository, value: &str) -> Result<ObjectId, String> {
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

#[cfg(feature = "git")]
fn first_parent(repository: &Repository, id: ObjectId) -> Result<ObjectId, String> {
    repository
        .commit_metadata(id)
        .map_err(|error| error.to_string())?
        .parents
        .first()
        .copied()
        .ok_or_else(|| format!("commit {id} has no parent"))
}
