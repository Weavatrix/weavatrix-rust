use crate::engine::RepositoryState;
use crate::operations::{optional_str, optional_u64};
use blazingly_json::{Value, json};
use std::collections::BTreeSet;
use weavatrix_graph::NodeKind;

pub(in crate::operations) fn change_impact(
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
    if let Some(value) = args.get("files") {
        let files = value
            .as_array()
            .ok_or_else(|| "files must be an array of strings".to_owned())?;
        return Ok(Some(
            files
                .iter()
                .map(|item| {
                    item.as_str()
                        .map(normalize_path)
                        .ok_or_else(|| "files must contain only strings".to_owned())
                })
                .collect::<Result<Vec<_>, String>>()?,
        ));
    }
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
