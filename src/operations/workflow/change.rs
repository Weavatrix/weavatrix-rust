use crate::engine::RepositoryState;
use crate::operations::{optional_str, optional_u64};
use blazingly_json::{Value, json};
use std::collections::BTreeSet;
use weavatrix_graph::NodeKind;

/// Tool-specific keys `change_impact` accepts (plus shared catalog keys).
pub(super) const CHANGE_IMPACT_KEYS: &[&str] = &[
    "base",
    "base_ref",
    "head_ref",
    "diff",
    "files",
    "target",
    "depth",
    "max_nodes",
    "precision",
    "max_references",
    "timeout_ms",
];

pub(in crate::operations) fn change_impact(
    state: &RepositoryState,
    args: &Value,
) -> Result<Value, String> {
    crate::operations::reject_unknown_arguments("change_impact", args, CHANGE_IMPACT_KEYS)?;
    change_impact_unchecked(state, args)
}

/// Same as `change_impact` without tool-key rejection, for composite callers that
/// forward a larger argument object (for example `verified_change`).
pub(super) fn change_impact_unchecked(
    state: &RepositoryState,
    args: &Value,
) -> Result<Value, String> {
    crate::operations::require_graph_precision(args)?;
    let explicit_head = optional_str(args, "head_ref")?;
    let requested = explicit_changed_files(args)?;
    let (git, files) = if explicit_head.is_some() {
        let git = crate::operations::history::changes(state, args)?;
        let files = requested.unwrap_or_else(|| changed_files(&git));
        (git, files)
    } else {
        worktree_changes(state, args, requested)?
    };
    let depth = optional_u64(args, "depth")?.unwrap_or(2);
    let max = optional_u64(args, "max_nodes")?.unwrap_or(40);
    let mut impacts = Vec::new();
    let mut seen = BTreeSet::new();
    for file in &files {
        let Some(node) = state
            .graph()
            .nodes()
            .iter()
            .find(|node| node.kind == NodeKind::File && node.label == *file)
        else {
            continue;
        };
        let result = crate::operations::graph::dependents(
            state,
            &json!({"label": node.id.as_str(), "depth": depth, "max_nodes": max}),
        )?;
        for dependent in result["dependents"].as_array().into_iter().flatten() {
            let node = &dependent["node"];
            if let Some(id) = node["id"].as_str()
                && seen.insert(id.to_owned())
            {
                impacts.push(node.clone());
            }
        }
    }
    Ok(json!({
        "status": "COMPLETE",
        "changed_files": files,
        "impacted_nodes": impacts,
        "git": git,
        "precision": "graph",
        "semantic_precision": "BOUNDED_STATIC",
        "coverage_evidence": {
            "present": false,
            "reason": "change_impact computes static graph reachability and does not consume measured test coverage"
        }
    }))
}

pub(super) fn worktree_changes(
    state: &RepositoryState,
    args: &Value,
    requested: Option<Vec<String>>,
) -> Result<(Value, Vec<String>), String> {
    let base = optional_str(args, "base_ref")?
        .or(optional_str(args, "base")?)
        .unwrap_or("HEAD");
    let files = requested.map_or_else(
        || crate::operations::history::worktree_changed_files(state, base),
        Ok,
    )?;
    let git = json!({
        "base": base,
        "head": "WORKTREE",
        "changes": files.iter().map(|path| {
            json!({"path": path, "kind": "worktree"})
        }).collect::<Vec<_>>()
    });
    Ok((git, files))
}

pub(super) fn explicit_changed_files(args: &Value) -> Result<Option<Vec<String>>, String> {
    let files = parse_files_array(args)?;
    let target = optional_str(args, "target")?.map(normalize_path);
    match (files, target) {
        (Some(files), Some(target)) => {
            if files == [target.as_str()] {
                Ok(Some(files))
            } else {
                Err(format!(
                    "target and files disagree: target={target:?}, files={files:?}"
                ))
            }
        }
        (Some(files), None) => Ok(Some(files)),
        (None, Some(target)) => Ok(Some(vec![target])),
        (None, None) => parse_diff_files(args),
    }
}

fn parse_files_array(args: &Value) -> Result<Option<Vec<String>>, String> {
    let Some(value) = args.get("files") else {
        return Ok(None);
    };
    let files = value
        .as_array()
        .ok_or_else(|| "files must be an array of strings".to_owned())?;
    Ok(Some(
        files
            .iter()
            .map(|item| {
                item.as_str()
                    .map(normalize_path)
                    .ok_or_else(|| "files must contain only strings".to_owned())
            })
            .collect::<Result<Vec<_>, String>>()?,
    ))
}

fn parse_diff_files(args: &Value) -> Result<Option<Vec<String>>, String> {
    let Some(diff) = optional_str(args, "diff")? else {
        return Ok(None);
    };
    let mut files = BTreeSet::new();
    for line in diff.lines() {
        let candidate = line
            .strip_prefix("+++ ")
            .or_else(|| line.strip_prefix("--- "))
            .or_else(|| {
                line.strip_prefix("diff --git ")
                    .and_then(|rest| rest.split_whitespace().nth(1))
            });
        let Some(path) = candidate else {
            continue;
        };
        let path = path.split('\t').next().unwrap_or(path);
        if path != "/dev/null" {
            let path = path
                .strip_prefix("a/")
                .or_else(|| path.strip_prefix("b/"))
                .unwrap_or(path);
            files.insert(normalize_path(path));
        }
    }
    Ok(Some(files.into_iter().collect()))
}

fn changed_files(git: &Value) -> Vec<String> {
    git["changes"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|change| change["path"].as_str().map(normalize_path))
        .collect()
}

fn normalize_path(path: &str) -> String {
    path.replace('\\', "/").trim_start_matches("./").to_owned()
}

#[cfg(test)]
mod tests {
    use super::{CHANGE_IMPACT_KEYS, explicit_changed_files};
    use crate::operations::reject_unknown_arguments;
    use blazingly_json::json;

    #[test]
    fn target_aliases_a_single_files_entry() {
        let files = explicit_changed_files(&json!({"target": "src/db.ts"})).unwrap();
        assert_eq!(files, Some(vec!["src/db.ts".to_owned()]));
    }

    #[test]
    fn agreeing_target_and_files_are_accepted() {
        let files = explicit_changed_files(&json!({"target": "src/db.ts", "files": ["src/db.ts"]}))
            .unwrap();
        assert_eq!(files, Some(vec!["src/db.ts".to_owned()]));
    }

    #[test]
    fn disagreeing_target_and_files_error() {
        let error = explicit_changed_files(&json!({"target": "src/a.ts", "files": ["src/b.ts"]}))
            .unwrap_err();
        assert!(error.contains("disagree"), "{error}");
    }

    #[test]
    fn unknown_argument_names_the_key_and_supported_set() {
        let error = reject_unknown_arguments(
            "change_impact",
            &json!({"max_reslts": 3}),
            CHANGE_IMPACT_KEYS,
        )
        .unwrap_err();
        assert!(error.contains("max_reslts"), "{error}");
        assert!(error.contains("files"), "{error}");
        assert!(error.contains("target"), "{error}");
    }
}
