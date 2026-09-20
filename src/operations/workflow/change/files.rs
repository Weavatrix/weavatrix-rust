use crate::engine::RepositoryState;
use crate::operations::optional_str;
use blazingly_json::{Value, json};
use std::collections::BTreeSet;

pub fn worktree_changes(
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

pub fn explicit_changed_files(args: &Value) -> Result<Option<Vec<String>>, String> {
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

pub(super) fn changed_files(git: &Value) -> Vec<String> {
    git["changes"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|change| change["path"].as_str().map(normalize_path))
        .collect()
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

fn normalize_path(path: &str) -> String {
    path.replace('\\', "/").trim_start_matches("./").to_owned()
}

#[cfg(test)]
mod tests {
    use super::super::CHANGE_IMPACT_KEYS;
    use super::explicit_changed_files;
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
