//! Structural Cargo manifest extraction. This parser reads TOML but never
//! executes Cargo, build scripts, proc macros, or workspace plugins.

use std::collections::BTreeMap;
use toml::{Table, Value};

use super::manifests::normalize_relative;
use super::model::{BuildCondition, BuildDependency, entity_id};

pub(super) struct CargoTarget {
    pub(super) kind: &'static str,
    pub(super) name: Option<String>,
    pub(super) path: Option<String>,
    pub(super) required_features: Vec<String>,
    pub(super) test: Option<bool>,
    pub(super) bench: Option<bool>,
    pub(super) doctest: Option<bool>,
    pub(super) harness: Option<bool>,
    pub(super) proc_macro: Option<bool>,
}

#[derive(Clone)]
pub(super) struct CargoDependency {
    pub(super) name: String,
    pub(super) native_name: String,
    pub(super) path: Option<String>,
    pub(super) scope: &'static str,
    pub(super) condition: Option<String>,
    pub(super) workspace_inherited: bool,
}

#[derive(Default)]
pub(super) struct CargoManifest {
    pub(super) valid: bool,
    pub(super) name: Option<String>,
    pub(super) workspace: bool,
    pub(super) workspace_members: Vec<String>,
    pub(super) workspace_excludes: Vec<String>,
    pub(super) workspace_default_members: Vec<String>,
    pub(super) workspace_dependencies: BTreeMap<String, CargoDependency>,
    pub(super) targets: Vec<CargoTarget>,
    pub(super) dependencies: Vec<CargoDependency>,
    pub(super) autolib: Option<bool>,
    pub(super) autobins: Option<bool>,
    pub(super) autotests: Option<bool>,
    pub(super) autobenches: Option<bool>,
    pub(super) autoexamples: Option<bool>,
}

pub(super) fn cargo_manifest(text: &str) -> CargoManifest {
    let Ok(root) = text.parse::<Table>() else {
        return CargoManifest::default();
    };
    let mut manifest = CargoManifest {
        valid: true,
        ..CargoManifest::default()
    };
    if let Some(package) = root.get("package").and_then(Value::as_table) {
        manifest.name = string(package, "name");
        manifest.autolib = boolean(package, "autolib");
        manifest.autobins = boolean(package, "autobins");
        manifest.autotests = boolean(package, "autotests");
        manifest.autobenches = boolean(package, "autobenches");
        manifest.autoexamples = boolean(package, "autoexamples");
    }
    if let Some(workspace) = root.get("workspace").and_then(Value::as_table) {
        manifest.workspace = true;
        manifest.workspace_members = strings(workspace.get("members"));
        manifest.workspace_excludes = strings(workspace.get("exclude"));
        manifest.workspace_default_members = strings(workspace.get("default-members"));
        manifest.workspace_dependencies = dependencies(
            workspace.get("dependencies"),
            "workspace-dependencies",
            None,
        )
        .into_iter()
        .map(|dependency| (dependency.name.clone(), dependency))
        .collect();
    }
    for (key, scope) in dependency_scopes() {
        manifest
            .dependencies
            .extend(dependencies(root.get(key), scope, None));
    }
    if let Some(targets) = root.get("target").and_then(Value::as_table) {
        for (condition, target) in targets {
            let Some(target) = target.as_table() else {
                continue;
            };
            for (key, scope) in dependency_scopes() {
                manifest
                    .dependencies
                    .extend(dependencies(target.get(key), scope, Some(condition)));
            }
        }
    }
    if let Some(target) = root.get("lib").and_then(Value::as_table) {
        manifest.targets.push(cargo_target("lib", target));
    }
    for (key, kind) in [
        ("bin", "bin"),
        ("bench", "bench"),
        ("test", "test"),
        ("example", "example"),
    ] {
        for target in root
            .get(key)
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(Value::as_table)
        {
            manifest.targets.push(cargo_target(kind, target));
        }
    }
    manifest
}

fn dependency_scopes() -> [(&'static str, &'static str); 3] {
    [
        ("dependencies", "dependencies"),
        ("dev-dependencies", "dev-dependencies"),
        ("build-dependencies", "build-dependencies"),
    ]
}

fn dependencies(
    value: Option<&Value>,
    scope: &'static str,
    condition: Option<&str>,
) -> Vec<CargoDependency> {
    value
        .and_then(Value::as_table)
        .into_iter()
        .flatten()
        .map(|(name, declaration)| {
            let table = declaration.as_table();
            CargoDependency {
                name: name.clone(),
                native_name: table
                    .and_then(|table| string(table, "package"))
                    .unwrap_or_else(|| name.clone()),
                path: table.and_then(|table| string(table, "path")),
                scope,
                condition: condition.map(str::to_owned),
                workspace_inherited: table
                    .and_then(|table| boolean(table, "workspace"))
                    .unwrap_or(false),
            }
        })
        .collect()
}

fn cargo_target(kind: &'static str, table: &Table) -> CargoTarget {
    CargoTarget {
        kind,
        name: string(table, "name"),
        path: string(table, "path"),
        required_features: strings(table.get("required-features")),
        test: boolean(table, "test"),
        bench: boolean(table, "bench"),
        doctest: boolean(table, "doctest"),
        harness: boolean(table, "harness"),
        proc_macro: boolean(table, "proc-macro"),
    }
}

fn string(table: &Table, key: &str) -> Option<String> {
    table.get(key)?.as_str().map(str::to_owned)
}

fn boolean(table: &Table, key: &str) -> Option<bool> {
    table.get(key)?.as_bool()
}

fn strings(value: Option<&Value>) -> Vec<String> {
    value
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .map(str::to_owned)
        .collect()
}

pub(super) fn cargo_default_target_path(
    kind: &str,
    name: Option<&str>,
    package_name: &str,
    exists: impl Fn(&str) -> bool,
) -> Option<String> {
    let name = name.unwrap_or(package_name);
    let candidates = match kind {
        "lib" => vec!["src/lib.rs".to_owned()],
        "bin" if name == package_name => vec!["src/main.rs".to_owned()],
        "bin" => vec![
            format!("src/bin/{name}.rs"),
            format!("src/bin/{name}/main.rs"),
        ],
        "test" | "bench" | "example" => {
            let directory = match kind {
                "test" => "tests",
                "bench" => "benches",
                _ => "examples",
            };
            vec![
                format!("{directory}/{name}.rs"),
                format!("{directory}/{name}/main.rs"),
            ]
        }
        _ => Vec::new(),
    };
    candidates.into_iter().find(|candidate| exists(candidate))
}

pub(super) fn resolve_dependencies(
    repository: &str,
    manifest: &str,
    member_dir: &str,
    parsed: &CargoManifest,
    workspace: Option<(&CargoManifest, &str)>,
) -> Vec<BuildDependency> {
    parsed
        .dependencies
        .iter()
        .map(|declared| {
            let inherited = declared.workspace_inherited;
            let inherited_value = workspace.and_then(|(root, _)| {
                inherited
                    .then(|| root.workspace_dependencies.get(&declared.name))
                    .flatten()
            });
            let resolved = inherited_value.unwrap_or(declared);
            let base = if inherited_value.is_some() {
                workspace.map_or(member_dir, |(_, root)| root)
            } else {
                member_dir
            };
            let target_dir = resolved
                .path
                .as_deref()
                .and_then(|path| normalize_relative(base, path));
            let member = target_dir.as_deref().map(|target| {
                entity_id(
                    repository,
                    "member",
                    &[
                        "cargo",
                        &if target.is_empty() {
                            "Cargo.toml".to_owned()
                        } else {
                            format!("{target}/Cargo.toml")
                        },
                    ],
                )
            });
            let condition = declared.condition.as_ref().map_or_else(
                || "UNCONDITIONAL_DECLARATION".to_owned(),
                |condition| format!("cargo_target:{condition}"),
            );
            let condition_ast = if declared.condition.is_some() {
                BuildCondition::atoms(std::slice::from_ref(&condition), true)
            } else {
                BuildCondition::Always
            };
            BuildDependency {
                id: entity_id(
                    repository,
                    "build-dependency",
                    &[manifest, &declared.name, declared.scope, &condition],
                ),
                name: declared.name.clone(),
                native_name: resolved.native_name.clone(),
                member,
                source_target: None,
                target: None,
                scope: declared.scope,
                condition,
                condition_ast,
                workspace_inherited: inherited,
                resolution: if inherited && inherited_value.is_none() {
                    "UNRESOLVED_WORKSPACE_INHERITANCE"
                } else if target_dir.is_some() {
                    "LOCAL_PATH"
                } else {
                    "EXTERNAL_DECLARATION"
                },
            }
        })
        .collect()
}
