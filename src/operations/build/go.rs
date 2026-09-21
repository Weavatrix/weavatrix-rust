//! Static Go module/package/import topology. Build constraints remain attached
//! to source membership and imports; the Go toolchain is never executed.

use super::model::{BuildCondition, BuildDependency, Member, SourceModule, entity_id};
use crate::engine::RepositoryState;
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

type Source = (String, Vec<String>, Vec<String>);

pub(super) fn details(
    state: &RepositoryState,
    root: &str,
    module_name: Option<&str>,
    member_id: &str,
) -> (Vec<SourceModule>, Vec<BuildDependency>) {
    let mut groups = BTreeMap::<(String, String), Vec<Source>>::new();
    for path in state
        .evidence()
        .paths()
        .filter(|path| is_go_source(root, path))
    {
        let Some(text) = state.evidence().text(path) else {
            continue;
        };
        let Some(package) = package_name(text) else {
            continue;
        };
        groups
            .entry((parent(path), package.to_owned()))
            .or_default()
            .push((path.to_owned(), conditions(path, text), imports(text)));
    }
    let modules = groups
        .iter()
        .map(|((path, name), sources)| source_module(state, path, name, sources))
        .collect::<Vec<_>>();
    let targets = module_name.map_or_else(BTreeMap::new, |module_name| {
        groups
            .keys()
            .filter(|(_, package)| !package.ends_with("_test"))
            .map(|(path, package)| {
                (
                    import_path(module_name, root, path),
                    module_id(state, path, package),
                )
            })
            .collect()
    });
    let mut dependencies = Vec::new();
    for ((path, package), sources) in &groups {
        let source = module_id(state, path, package);
        let scope = if package.ends_with("_test") {
            "go_test_import"
        } else {
            "go_import"
        };
        let mut seen = BTreeSet::new();
        for (_, source_conditions, imports) in sources {
            for imported in imports {
                if !seen.insert((imported.clone(), source_conditions.clone())) {
                    continue;
                }
                let target = targets.get(imported).cloned();
                let condition = if source_conditions.is_empty() {
                    "UNCONDITIONAL_DECLARATION".to_owned()
                } else {
                    source_conditions.join(" && ")
                };
                dependencies.push(BuildDependency {
                    id: entity_id(
                        &state.snapshot().repository,
                        "build-dependency",
                        &["go", &source, imported, &condition],
                    ),
                    name: imported.clone(),
                    native_name: imported.clone(),
                    member: target.as_ref().map(|_| member_id.to_owned()),
                    source_target: Some(source.clone()),
                    target,
                    scope,
                    condition: condition.clone(),
                    condition_ast: BuildCondition::atoms(source_conditions, true),
                    workspace_inherited: false,
                    resolution: if targets.contains_key(imported) {
                        "LOCAL_MODULE"
                    } else {
                        "EXTERNAL_IMPORT"
                    },
                });
            }
        }
    }
    (modules, dependencies)
}

pub(super) fn link_members(members: &mut [Member]) {
    let mut targets = BTreeMap::new();
    for member in members.iter() {
        let Some(module_name) = member.name.as_deref() else {
            continue;
        };
        for module in &member.modules {
            if module.kind != "go_external_test_package" {
                targets.insert(
                    import_path(module_name, &member.path, &module.path),
                    (member.id.clone(), module.id.clone()),
                );
            }
        }
    }
    for member in members {
        for dependency in &mut member.dependencies {
            if dependency.resolution != "EXTERNAL_IMPORT" {
                continue;
            }
            if let Some((target_member, target)) = targets.get(&dependency.native_name) {
                dependency.member = Some(target_member.clone());
                dependency.target = Some(target.clone());
                dependency.resolution = "LOCAL_WORKSPACE_MODULE";
            }
        }
        member.internal_dependencies = member
            .dependencies
            .iter()
            .filter(|dependency| dependency.target.is_some())
            .cloned()
            .collect();
    }
}

fn source_module(
    state: &RepositoryState,
    path: &str,
    name: &str,
    sources: &[Source],
) -> SourceModule {
    let source_conditions = sources
        .iter()
        .filter(|(_, conditions, _)| !conditions.is_empty())
        .map(|(source, conditions, _)| (source.clone(), conditions.clone()))
        .collect::<BTreeMap<_, _>>();
    let conditions = source_conditions
        .values()
        .flatten()
        .cloned()
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect::<Vec<_>>();
    let unconditional = sources
        .iter()
        .any(|(_, conditions, _)| conditions.is_empty());
    SourceModule {
        id: module_id(state, path, name),
        kind: if name == "main" {
            "go_command"
        } else if name.ends_with("_test") {
            "go_external_test_package"
        } else {
            "go_package"
        },
        name: name.to_owned(),
        path: path.to_owned(),
        source_files: sources.iter().map(|(path, _, _)| path.clone()).collect(),
        applicability: if source_conditions.is_empty() {
            "ALL_SCENARIOS"
        } else if unconditional {
            "ALL_WITH_CONDITIONAL_SOURCES"
        } else {
            "CONDITIONAL_PLATFORM"
        },
        condition_ast: if unconditional {
            BuildCondition::Always
        } else {
            BuildCondition::atoms(&conditions, false)
        },
        source_condition_ast: source_conditions
            .iter()
            .map(|(source, conditions)| (source.clone(), BuildCondition::atoms(conditions, true)))
            .collect(),
        source_conditions,
        conditions,
    }
}

fn imports(text: &str) -> Vec<String> {
    let mut result = Vec::new();
    let mut block = false;
    for raw in text.lines() {
        let line = raw.split("//").next().unwrap_or_default().trim();
        if block {
            if line == ")" {
                block = false;
            } else if let Some(imported) = quoted(line) {
                result.push(imported);
            }
        } else if let Some(value) = line.strip_prefix("import ") {
            if value.trim() == "(" {
                block = true;
            } else if let Some(imported) = quoted(value) {
                result.push(imported);
            }
        }
    }
    result
}

fn quoted(text: &str) -> Option<String> {
    for quote in ['"', '`'] {
        if let Some((_, rest)) = text.split_once(quote)
            && let Some((value, _)) = rest.split_once(quote)
        {
            return Some(value.to_owned());
        }
    }
    None
}

fn conditions(path: &str, text: &str) -> Vec<String> {
    let mut result = text
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
            result.insert(format!("platform:{suffix}"));
        }
    }
    result.into_iter().collect()
}

fn package_name(text: &str) -> Option<&str> {
    text.lines()
        .map(str::trim)
        .find_map(|line| line.strip_prefix("package "))
        .and_then(|name| name.split_whitespace().next())
}

fn import_path(module: &str, root: &str, path: &str) -> String {
    let relative = path
        .strip_prefix(&format!("{root}/"))
        .unwrap_or(path)
        .trim_matches('/');
    if relative.is_empty() {
        module.to_owned()
    } else {
        format!("{module}/{relative}")
    }
}

fn module_id(state: &RepositoryState, path: &str, package: &str) -> String {
    entity_id(
        &state.snapshot().repository,
        "module",
        &["go", path, package],
    )
}

fn is_go_source(root: &str, path: &str) -> bool {
    (root.is_empty() || path.starts_with(&format!("{root}/")))
        && Path::new(path)
            .extension()
            .is_some_and(|extension| extension.eq_ignore_ascii_case("go"))
}

fn parent(path: &str) -> String {
    path.rsplit_once('/')
        .map_or(String::new(), |(directory, _)| directory.to_owned())
}
