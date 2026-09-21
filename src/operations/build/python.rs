use super::ManifestIndex;
use super::model::{
    BuildCondition, BuildDependency, BuildTask, Member, SourceModule, Workspace, entity_id,
};
use crate::engine::RepositoryState;
use std::collections::BTreeMap;
use std::path::Path;
use toml::{Table, Value};

pub(super) fn distributions(
    state: &RepositoryState,
    index: &ManifestIndex,
    workspaces: &mut Vec<Workspace>,
) {
    for manifest in super::locate(state, index, "pyproject.toml") {
        let Some(text) = super::read_manifest(state, &manifest) else {
            continue;
        };
        let project = project(&text);
        let Some(name) = project.name else { continue };
        let root = super::parent_dir(&manifest);
        let mut tasks = project
            .scripts
            .into_iter()
            .map(|(script, entry_point)| BuildTask {
                id: entity_id(
                    &state.snapshot().repository,
                    "task",
                    &["python-entry-point", &manifest, &script],
                ),
                kind: "entry_point",
                name: script,
                command: entry_point,
                invokes: Vec::new(),
                cycle: false,
                argument_forwarding: "NOT_APPLICABLE",
            })
            .collect::<Vec<_>>();
        if project.pytest_configured {
            tasks.push(BuildTask {
                id: entity_id(
                    &state.snapshot().repository,
                    "task",
                    &["python-test-config", &manifest, "pytest"],
                ),
                kind: "test_configuration",
                name: "pytest".to_owned(),
                command: "declarative pytest configuration".to_owned(),
                invokes: Vec::new(),
                cycle: false,
                argument_forwarding: "NOT_APPLICABLE",
            });
        }
        let dependencies = project
            .dependencies
            .into_iter()
            .map(|dependency| BuildDependency {
                id: entity_id(
                    &state.snapshot().repository,
                    "build-dependency",
                    &["python", &manifest, &dependency.name, dependency.scope],
                ),
                name: dependency.name.clone(),
                native_name: dependency.declaration,
                member: None,
                source_target: None,
                target: None,
                scope: dependency.scope,
                condition: "UNCONDITIONAL_DECLARATION".to_owned(),
                condition_ast: BuildCondition::Always,
                workspace_inherited: false,
                resolution: if dependency.local {
                    "LOCAL_PATH_UNRESOLVED"
                } else {
                    "EXTERNAL_DECLARATION"
                },
            })
            .collect::<Vec<_>>();
        let member = Member {
            id: entity_id(
                &state.snapshot().repository,
                "member",
                &["python", &manifest],
            ),
            kind: "python_distribution",
            name: Some(name),
            path: root.clone(),
            manifest: manifest.clone(),
            targets: Vec::new(),
            modules: modules(state, &root),
            tasks,
            dependencies,
            internal_dependencies: Vec::new(),
        };
        let default_members = vec![member.id.clone()];
        workspaces.push(Workspace {
            id: entity_id(
                &state.snapshot().repository,
                "workspace",
                &["python", &manifest],
            ),
            ecosystem: "python",
            aggregator: manifest,
            default_members,
            excluded_paths: Vec::new(),
            members: vec![member],
        });
    }
}

fn modules(state: &RepositoryState, root: &str) -> Vec<SourceModule> {
    let mut groups = BTreeMap::<String, Vec<String>>::new();
    for path in state.evidence().paths().filter(|path| {
        contains(root, path)
            && Path::new(path)
                .extension()
                .is_some_and(|extension| extension.eq_ignore_ascii_case("py"))
    }) {
        let relative = relative_to(root, path);
        let directory = parent(relative);
        let key = if path.ends_with("/__init__.py") {
            directory
        } else if directory.is_empty() {
            relative.trim_end_matches(".py").to_owned()
        } else {
            directory
        };
        groups.entry(key).or_default().push(path.to_owned());
    }
    groups
        .into_iter()
        .map(|(path, mut source_files)| {
            source_files.sort();
            let confirmed = source_files
                .iter()
                .any(|source| source.ends_with("/__init__.py"));
            SourceModule {
                id: entity_id(
                    &state.snapshot().repository,
                    "module",
                    &["python", root, &path],
                ),
                kind: if confirmed {
                    "python_import_package"
                } else {
                    "python_namespace_or_module"
                },
                name: path.replace('/', "."),
                path: join(root, &path),
                source_files,
                conditions: Vec::new(),
                source_conditions: BTreeMap::new(),
                condition_ast: BuildCondition::Always,
                source_condition_ast: BTreeMap::new(),
                applicability: if confirmed {
                    "STATIC_PACKAGE_EVIDENCE"
                } else {
                    "NAMESPACE_CANDIDATE"
                },
            }
        })
        .collect()
}

struct Project {
    name: Option<String>,
    scripts: Vec<(String, String)>,
    dependencies: Vec<PythonDependency>,
    pytest_configured: bool,
}

struct PythonDependency {
    name: String,
    declaration: String,
    scope: &'static str,
    local: bool,
}

fn project(text: &str) -> Project {
    let Ok(root) = text.parse::<Table>() else {
        return Project {
            name: None,
            scripts: Vec::new(),
            dependencies: Vec::new(),
            pytest_configured: false,
        };
    };
    let project = root.get("project").and_then(Value::as_table);
    let name = project
        .and_then(|project| project.get("name"))
        .and_then(Value::as_str)
        .map(str::to_owned);
    let mut scripts = Vec::new();
    for (name, entry) in project
        .and_then(|project| project.get("scripts"))
        .and_then(Value::as_table)
        .into_iter()
        .flatten()
    {
        if let Some(entry) = entry.as_str() {
            scripts.push((name.clone(), entry.to_owned()));
        }
    }
    let mut dependencies = Vec::new();
    for declaration in project
        .and_then(|project| project.get("dependencies"))
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
    {
        if let Some(name) = pep508_name(declaration) {
            dependencies.push(PythonDependency {
                name,
                declaration: declaration.to_owned(),
                scope: "dependencies",
                local: declaration.contains(" @ file:"),
            });
        }
    }
    if let Some(optional) = project
        .and_then(|project| project.get("optional-dependencies"))
        .and_then(Value::as_table)
    {
        for values in optional.values() {
            for declaration in values
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(Value::as_str)
            {
                if let Some(name) = pep508_name(declaration) {
                    dependencies.push(PythonDependency {
                        name,
                        declaration: declaration.to_owned(),
                        scope: "optional-dependencies",
                        local: declaration.contains(" @ file:"),
                    });
                }
            }
        }
    }
    let pytest_configured = root
        .get("tool")
        .and_then(Value::as_table)
        .and_then(|tool| tool.get("pytest"))
        .and_then(Value::as_table)
        .is_some_and(|pytest| pytest.contains_key("ini_options"));
    Project {
        name,
        scripts,
        dependencies,
        pytest_configured,
    }
}

pub(super) fn valid_manifest(text: &str) -> bool {
    text.parse::<Table>().is_ok()
}

fn pep508_name(declaration: &str) -> Option<String> {
    let name = declaration
        .trim()
        .chars()
        .take_while(|character| character.is_ascii_alphanumeric() || "-_.".contains(*character))
        .collect::<String>();
    (!name.is_empty()).then_some(name)
}

fn contains(root: &str, path: &str) -> bool {
    root.is_empty() || path == root || path.starts_with(&format!("{root}/"))
}

fn relative_to<'a>(root: &str, path: &'a str) -> &'a str {
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

fn join(root: &str, path: &str) -> String {
    if root.is_empty() {
        path.to_owned()
    } else {
        format!("{root}/{path}")
    }
}
