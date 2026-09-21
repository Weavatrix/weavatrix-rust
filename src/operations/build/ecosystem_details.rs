//! Static ecosystem details that do not execute build tools or plugins.

use super::manifests::normalize_relative;
use super::model::{
    BuildCondition, BuildDependency, BuildTarget, Member, TargetOptions, entity_id,
};
use crate::engine::RepositoryState;
use blazingly_json::Value;
use std::collections::BTreeMap;
use std::path::Path;

pub(super) fn typescript_projects(state: &RepositoryState, member: &mut Member) {
    let configs = state
        .evidence()
        .paths()
        .filter(|path| contains(&member.path, path) && is_tsconfig(path))
        .map(str::to_owned)
        .collect::<Vec<_>>();
    let ids = configs
        .iter()
        .map(|path| {
            (
                path.clone(),
                entity_id(
                    &state.snapshot().repository,
                    "target",
                    &["typescript", path],
                ),
            )
        })
        .collect::<BTreeMap<_, _>>();
    for path in &configs {
        let Some(text) = state.evidence().text(path) else {
            continue;
        };
        let Ok(document) = blazingly_json::from_str::<Value>(text) else {
            continue;
        };
        let target_id = ids[path].clone();
        let relative_path = relative_to(&member.path, path).to_owned();
        member.targets.push(BuildTarget {
            id: target_id.clone(),
            kind: "ts_project",
            name: Some(path.clone()),
            path: Some(relative_path),
            implicit: false,
            required_features: Vec::new(),
            source_patterns: ts_source_patterns(&document),
            conditions: Vec::new(),
            condition_ast: BuildCondition::Always,
            options: TargetOptions {
                composite: document["compilerOptions"]["composite"].as_bool(),
                extends: document["extends"].as_str().map(str::to_owned),
                ..TargetOptions::default()
            },
            applicability: "DECLARED_STATIC_CONFIG",
        });
        let references = document["references"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|item| item["path"].as_str())
            .map(|path| (path, "ts_project_reference"));
        let extends = document["extends"]
            .as_str()
            .into_iter()
            .map(|path| (path, "tsconfig_extends"));
        for (reference, scope) in references.chain(extends) {
            let Some(target_path) = referenced_tsconfig(path, reference) else {
                continue;
            };
            let target = ids.get(&target_path).cloned();
            let dependency = BuildDependency {
                id: entity_id(
                    &state.snapshot().repository,
                    "build-dependency",
                    &[path, &target_path, scope],
                ),
                name: target_path.clone(),
                native_name: "typescript-project-reference".to_owned(),
                member: target.as_ref().map(|_| member.id.clone()),
                source_target: Some(target_id.clone()),
                target,
                scope,
                condition: "UNCONDITIONAL_DECLARATION".to_owned(),
                condition_ast: BuildCondition::Always,
                workspace_inherited: false,
                resolution: if ids.contains_key(&target_path) {
                    "LOCAL_TARGET"
                } else if reference.starts_with('.') {
                    "LOCAL_CONFIG_UNRESOLVED"
                } else {
                    "EXTERNAL_CONFIG"
                },
            };
            member.dependencies.push(dependency.clone());
            member.internal_dependencies.push(dependency);
        }
    }
}

fn ts_source_patterns(document: &Value) -> Vec<String> {
    let mut patterns = Vec::new();
    for key in ["files", "include"] {
        patterns.extend(
            document[key]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(Value::as_str)
                .map(str::to_owned),
        );
    }
    patterns.extend(
        document["exclude"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
            .map(|pattern| format!("!{pattern}")),
    );
    if !patterns.iter().any(|pattern| !pattern.starts_with('!')) {
        patterns.push("**/*".to_owned());
    }
    patterns
}

pub(super) fn target_owns_source(target: &BuildTarget, member: &str, file: &str) -> bool {
    if target.kind != "ts_project"
        || !Path::new(file).extension().is_some_and(|extension| {
            matches!(extension.to_str(), Some("ts" | "tsx" | "js" | "jsx"))
        })
    {
        return false;
    }
    let Some(config) = target.path.as_deref() else {
        return false;
    };
    let config_dir = parent(config);
    let member_relative = relative_to(member, file);
    let relative = relative_to(&config_dir, member_relative);
    target
        .source_patterns
        .iter()
        .filter(|pattern| !pattern.starts_with('!'))
        .any(|pattern| path_glob(pattern, relative))
        && !target
            .source_patterns
            .iter()
            .filter_map(|pattern| pattern.strip_prefix('!'))
            .any(|pattern| path_glob(pattern, relative))
}

fn path_glob(pattern: &str, path: &str) -> bool {
    fn matches(pattern: &[&str], path: &[&str]) -> bool {
        match (pattern.first(), path.first()) {
            (None, None) => true,
            (Some(&"**"), _) => {
                matches(&pattern[1..], path) || (!path.is_empty() && matches(pattern, &path[1..]))
            }
            (Some(pattern_segment), Some(path_segment))
                if segment_glob(pattern_segment, path_segment) =>
            {
                matches(&pattern[1..], &path[1..])
            }
            _ => false,
        }
    }
    matches(
        &pattern
            .trim_start_matches("./")
            .split('/')
            .collect::<Vec<_>>(),
        &path.split('/').collect::<Vec<_>>(),
    )
}

fn segment_glob(pattern: &str, value: &str) -> bool {
    let parts = pattern.split('*').collect::<Vec<_>>();
    if parts.len() == 1 {
        return pattern == value;
    }
    let mut offset = 0;
    for (index, part) in parts.iter().enumerate() {
        if part.is_empty() {
            continue;
        }
        let Some(found) = value[offset..].find(part) else {
            return false;
        };
        if index == 0 && found != 0 {
            return false;
        }
        offset += found + part.len();
    }
    pattern.ends_with('*') || offset == value.len()
}

fn referenced_tsconfig(source: &str, reference: &str) -> Option<String> {
    let mut target = normalize_relative(&parent(source), reference)?;
    if Path::new(&target).extension().is_none() {
        target = join(&target, "tsconfig.json");
    }
    Some(target)
}

fn is_tsconfig(path: &str) -> bool {
    let name = path.rsplit('/').next().unwrap_or(path);
    name.starts_with("tsconfig")
        && Path::new(name)
            .extension()
            .is_some_and(|extension| extension.eq_ignore_ascii_case("json"))
}

pub(super) fn contains(root: &str, path: &str) -> bool {
    root.is_empty() || path == root || path.starts_with(&format!("{root}/"))
}

pub(super) fn relative_to<'a>(root: &str, path: &'a str) -> &'a str {
    if root.is_empty() {
        path
    } else {
        path.strip_prefix(&format!("{root}/")).unwrap_or(path)
    }
}

fn parent(path: &str) -> String {
    path.rsplit_once('/')
        .map_or(String::new(), |(directory, _)| directory.to_owned())
}

pub(super) fn join(root: &str, path: &str) -> String {
    if root.is_empty() {
        path.to_owned()
    } else {
        format!("{root}/{path}")
    }
}

pub(super) fn cargo_owns_source(kind: &str, target_path: &str, file: &str) -> bool {
    matches!(kind, "lib" | "bin") && target_path.starts_with("src/") && file.starts_with("src/")
}
