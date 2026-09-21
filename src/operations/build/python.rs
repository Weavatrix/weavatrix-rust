use super::ManifestIndex;
use super::model::{BuildCondition, BuildTask, Member, SourceModule, Workspace, entity_id};
use crate::engine::RepositoryState;
use std::collections::BTreeMap;
use std::path::Path;

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
        let tasks = project
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
            })
            .collect();
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
}

fn project(text: &str) -> Project {
    let mut section = "";
    let mut name = None;
    let mut scripts = Vec::new();
    for raw in text.lines() {
        let line = raw.split('#').next().unwrap_or_default().trim();
        if line.starts_with('[') && line.ends_with(']') {
            section = line.trim_matches(['[', ']']);
        } else if let Some((key, value)) = line.split_once('=') {
            let value = value.trim().trim_matches(['"', '\'']).to_owned();
            if section == "project" && key.trim() == "name" {
                name = Some(value);
            } else if section == "project.scripts" {
                scripts.push((key.trim().to_owned(), value));
            }
        }
    }
    Project { name, scripts }
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
