//! Static ecosystem details that do not execute build tools or plugins.

use super::manifests::normalize_relative;
use super::model::{
    BuildCondition, BuildDependency, BuildTarget, Member, SourceModule, TargetOptions, entity_id,
};
use crate::engine::RepositoryState;
use blazingly_json::Value;
use std::collections::{BTreeMap, BTreeSet};
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
        member.targets.push(BuildTarget {
            id: target_id.clone(),
            kind: "ts_project",
            name: Some(path.clone()),
            path: Some(path.clone()),
            implicit: false,
            required_features: Vec::new(),
            source_patterns: ts_source_patterns(&document),
            conditions: Vec::new(),
            condition_ast: BuildCondition::Always,
            options: TargetOptions::default(),
            applicability: "DECLARED_STATIC_CONFIG",
        });
        for reference in document["references"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|item| item["path"].as_str())
        {
            let Some(target_path) = referenced_tsconfig(path, reference) else {
                continue;
            };
            let Some(target) = ids.get(&target_path) else {
                continue;
            };
            member.internal_dependencies.push(BuildDependency {
                id: entity_id(
                    &state.snapshot().repository,
                    "build-dependency",
                    &[path, &target_path, "ts_project_reference"],
                ),
                name: target_path,
                member: Some(member.id.clone()),
                source_target: Some(target_id.clone()),
                target: Some(target.clone()),
                scope: "ts_project_reference",
                condition: "UNCONDITIONAL_DECLARATION",
                condition_ast: BuildCondition::Always,
            });
        }
    }
}

pub(super) fn go_packages(state: &RepositoryState, root: &str) -> Vec<SourceModule> {
    let mut groups = BTreeMap::<(String, String), Vec<(String, Vec<String>)>>::new();
    for path in state.evidence().paths().filter(|path| {
        contains(root, path)
            && Path::new(path)
                .extension()
                .is_some_and(|extension| extension.eq_ignore_ascii_case("go"))
    }) {
        let Some(text) = state.evidence().text(path) else {
            continue;
        };
        let Some(package) = go_package_name(text) else {
            continue;
        };
        let directory = parent(path);
        groups
            .entry((directory, package.to_owned()))
            .or_default()
            .push((path.to_owned(), go_conditions(path, text)));
    }
    groups
        .into_iter()
        .map(|((path, name), sources)| {
            let source_conditions = sources
                .iter()
                .filter(|(_, conditions)| !conditions.is_empty())
                .cloned()
                .collect::<BTreeMap<_, _>>();
            let mut source_files = sources
                .iter()
                .map(|(source, _)| source.clone())
                .collect::<Vec<_>>();
            source_files.sort();
            let conditions = source_conditions
                .values()
                .flatten()
                .cloned()
                .collect::<BTreeSet<_>>()
                .into_iter()
                .collect::<Vec<_>>();
            let unconditional = sources.iter().any(|(_, conditions)| conditions.is_empty());
            let condition_ast = if unconditional {
                BuildCondition::Always
            } else {
                BuildCondition::atoms(&conditions, false)
            };
            let source_condition_ast = source_conditions
                .iter()
                .map(|(source, conditions)| {
                    (source.clone(), BuildCondition::atoms(conditions, true))
                })
                .collect();
            SourceModule {
                id: entity_id(
                    &state.snapshot().repository,
                    "module",
                    &["go", &path, &name],
                ),
                kind: if name == "main" {
                    "go_command"
                } else if name.ends_with("_test") {
                    "go_external_test_package"
                } else {
                    "go_package"
                },
                name,
                path,
                source_files,
                applicability: if source_conditions.is_empty() {
                    "ALL_SCENARIOS"
                } else if unconditional {
                    "ALL_WITH_CONDITIONAL_SOURCES"
                } else {
                    "CONDITIONAL_PLATFORM"
                },
                conditions,
                condition_ast,
                source_condition_ast,
                source_conditions,
            }
        })
        .collect()
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
    patterns
}

fn referenced_tsconfig(source: &str, reference: &str) -> Option<String> {
    let mut target = normalize_relative(&parent(source), reference)?;
    if Path::new(&target).extension().is_none() {
        target = join(&target, "tsconfig.json");
    }
    Some(target)
}

fn go_package_name(text: &str) -> Option<&str> {
    text.lines()
        .map(str::trim)
        .find_map(|line| line.strip_prefix("package "))
        .and_then(|name| name.split_whitespace().next())
}

fn go_conditions(path: &str, text: &str) -> Vec<String> {
    let mut conditions = text
        .lines()
        .take(20)
        .filter_map(|line| line.trim().strip_prefix("//go:build "))
        .map(|condition| format!("go_build:{condition}"))
        .collect::<BTreeSet<_>>();
    let stem = Path::new(path)
        .file_stem()
        .and_then(|value| value.to_str())
        .unwrap_or_default();
    for suffix in ["linux", "windows", "darwin", "freebsd", "amd64", "arm64"] {
        if stem.ends_with(&format!("_{suffix}")) {
            conditions.insert(format!("platform:{suffix}"));
        }
    }
    conditions.into_iter().collect()
}

fn is_tsconfig(path: &str) -> bool {
    let name = path.rsplit('/').next().unwrap_or(path);
    name.starts_with("tsconfig")
        && Path::new(name)
            .extension()
            .is_some_and(|extension| extension.eq_ignore_ascii_case("json"))
}

fn contains(root: &str, path: &str) -> bool {
    root.is_empty() || path == root || path.starts_with(&format!("{root}/"))
}

fn parent(path: &str) -> String {
    path.rsplit_once('/')
        .map_or(String::new(), |(directory, _)| directory.to_owned())
}

fn join(root: &str, path: &str) -> String {
    if root.is_empty() {
        path.to_owned()
    } else {
        format!("{root}/{path}")
    }
}
