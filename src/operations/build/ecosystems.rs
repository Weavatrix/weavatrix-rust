use super::manifests::{cargo_manifest, normalize_relative, npm_package};
use super::model::{Member, Workspace, entity_id};
use super::{
    ManifestIndex, dir_is_member, ecosystem_details, locate, manifests, parent_dir, read_manifest,
    render,
};
use crate::engine::RepositoryState;
use std::collections::{BTreeMap, BTreeSet};

fn id(state: &RepositoryState, kind: &str, parts: &[&str]) -> String {
    entity_id(&state.snapshot().repository, kind, parts)
}

pub(super) fn npm_workspaces(
    state: &RepositoryState,
    index: &ManifestIndex,
    workspaces: &mut Vec<Workspace>,
    claimed: &mut BTreeSet<String>,
) {
    let mut aggregators = Vec::new();
    for manifest in locate(state, index, "package.json") {
        if let Some(patterns) = read_manifest(state, &manifest)
            .as_deref()
            .and_then(manifests::npm_workspace_patterns)
        {
            aggregators.push((manifest, patterns));
        }
    }
    for manifest in locate(state, index, "pnpm-workspace.yaml") {
        if let Some(text) = read_manifest(state, &manifest) {
            aggregators.push((manifest, manifests::yaml_packages(&text)));
        }
    }
    for manifest in locate(state, index, "lerna.json") {
        if let Some(text) = read_manifest(state, &manifest) {
            aggregators.push((manifest, manifests::json_packages(&text)));
        }
    }
    for (aggregator, patterns) in aggregators {
        let aggregator_dir = parent_dir(&aggregator);
        let mut members = Vec::new();
        for manifest in locate(state, index, "package.json") {
            let dir = parent_dir(&manifest);
            if dir_is_member(&aggregator_dir, &patterns, &dir) {
                claimed.insert(format!("npm:{manifest}"));
                members.push(npm_member(state, &manifest, &dir));
            }
        }
        link_npm_members(&state.snapshot().repository, &mut members);
        let default_members = members.iter().map(|member| member.id.clone()).collect();
        let excluded_paths = patterns
            .iter()
            .filter_map(|pattern| pattern.strip_prefix('!').map(str::to_owned))
            .collect();
        workspaces.push(Workspace {
            id: id(state, "workspace", &["npm", &aggregator]),
            ecosystem: "npm",
            aggregator,
            default_members,
            excluded_paths,
            members,
        });
    }
}

pub(super) fn npm_member(state: &RepositoryState, manifest: &str, dir: &str) -> Member {
    let package = read_manifest(state, manifest)
        .as_deref()
        .map_or_else(|| npm_package(""), npm_package);
    let dependencies = render::npm_dependencies(
        &state.snapshot().repository,
        manifest,
        &package.dependencies,
    );
    let mut member = Member {
        id: id(state, "member", &["npm", manifest]),
        kind: "npm_package",
        name: package.name,
        path: dir.to_owned(),
        manifest: manifest.to_owned(),
        targets: Vec::new(),
        modules: Vec::new(),
        tasks: render::script_tasks(&state.snapshot().repository, manifest, &package.scripts),
        dependencies: dependencies.clone(),
        internal_dependencies: dependencies,
    };
    ecosystem_details::typescript_projects(state, &mut member);
    member
}

/// Keeps only dependencies whose name is another member of the same
/// workspace, and stamps that member's directory on the edge.
fn link_npm_members(repository: &str, members: &mut [Member]) {
    let names = members
        .iter()
        .filter_map(|member| Some((member.name.clone()?, member.manifest.clone())))
        .collect::<BTreeMap<_, _>>();
    for member in members.iter_mut() {
        for dependency in &mut member.dependencies {
            let _ = names.get(&dependency.name).is_some_and(|manifest| {
                dependency.member = Some(entity_id(repository, "member", &["npm", manifest]));
                dependency.resolution = "LOCAL_WORKSPACE_MEMBER";
                true
            });
        }
        member.internal_dependencies = member
            .dependencies
            .iter()
            .filter(|dependency| dependency.member.is_some())
            .cloned()
            .collect();
    }
}

pub(super) fn cargo_workspaces(
    state: &RepositoryState,
    index: &ManifestIndex,
    workspaces: &mut Vec<Workspace>,
    claimed: &mut BTreeSet<String>,
) {
    for aggregator in locate(state, index, "Cargo.toml") {
        let Some(text) = read_manifest(state, &aggregator) else {
            continue;
        };
        let parsed = cargo_manifest(&text);
        if !parsed.workspace {
            continue;
        }
        let aggregator_dir = parent_dir(&aggregator);
        let mut members = Vec::new();
        if parsed.name.is_some() {
            claimed.insert(format!("cargo:{aggregator}"));
            members.push(cargo_member(
                state,
                index,
                &aggregator,
                &aggregator_dir,
                Some((&parsed, &aggregator_dir)),
            ));
        }
        for manifest in locate(state, index, "Cargo.toml") {
            let dir = parent_dir(&manifest);
            if !dir_is_member(&aggregator_dir, &parsed.workspace_members, &dir)
                || dir_is_member(&aggregator_dir, &parsed.workspace_excludes, &dir)
            {
                continue;
            }
            claimed.insert(format!("cargo:{manifest}"));
            members.push(cargo_member(
                state,
                index,
                &manifest,
                &dir,
                Some((&parsed, &aggregator_dir)),
            ));
        }
        let dirs = members
            .iter()
            .map(|member| member.id.clone())
            .collect::<BTreeSet<_>>();
        for member in &mut members {
            member.internal_dependencies = member
                .dependencies
                .iter()
                .filter(|dependency| {
                    dependency
                        .member
                        .as_ref()
                        .is_some_and(|id| dirs.contains(id))
                })
                .cloned()
                .collect();
        }
        let default_members = if parsed.workspace_default_members.is_empty() {
            members.iter().map(|member| member.id.clone()).collect()
        } else {
            members
                .iter()
                .filter(|member| {
                    dir_is_member(
                        &aggregator_dir,
                        &parsed.workspace_default_members,
                        &member.path,
                    )
                })
                .map(|member| member.id.clone())
                .collect()
        };
        workspaces.push(Workspace {
            id: id(state, "workspace", &["cargo", &aggregator]),
            ecosystem: "cargo",
            aggregator,
            default_members,
            excluded_paths: parsed.workspace_excludes.clone(),
            members,
        });
    }
}

pub(super) fn cargo_member(
    state: &RepositoryState,
    index: &ManifestIndex,
    manifest: &str,
    dir: &str,
    workspace: Option<(&super::cargo_manifest::CargoManifest, &str)>,
) -> Member {
    let parsed = read_manifest(state, manifest)
        .as_deref()
        .map(cargo_manifest)
        .unwrap_or_default();
    let dependencies = super::cargo_manifest::resolve_dependencies(
        &state.snapshot().repository,
        manifest,
        dir,
        &parsed,
        workspace,
    );
    let internal_dependencies = dependencies
        .iter()
        .filter(|dependency| dependency.member.is_some())
        .cloned()
        .collect();
    Member {
        id: id(state, "member", &["cargo", manifest]),
        kind: "cargo_package",
        name: parsed.name.clone(),
        path: dir.to_owned(),
        manifest: manifest.to_owned(),
        targets: render::cargo_targets(&state.snapshot().repository, &parsed, index, dir, manifest),
        modules: Vec::new(),
        tasks: Vec::new(),
        dependencies,
        internal_dependencies,
    }
}

pub(super) fn go_workspaces(
    state: &RepositoryState,
    index: &ManifestIndex,
    workspaces: &mut Vec<Workspace>,
    claimed: &mut BTreeSet<String>,
) {
    for aggregator in locate(state, index, "go.work") {
        let Some(text) = read_manifest(state, &aggregator) else {
            continue;
        };
        let aggregator_dir = parent_dir(&aggregator);
        let mut members: Vec<Member> = manifests::go_work_uses(&text)
            .iter()
            .filter_map(|used| {
                let dir = normalize_relative(&aggregator_dir, used)?;
                let manifest = if dir.is_empty() {
                    "go.mod".to_owned()
                } else {
                    format!("{dir}/go.mod")
                };
                let exists = index.contains_file(&manifest);
                exists.then(|| {
                    claimed.insert(format!("go:{manifest}"));
                    go_member(state, manifest, &dir)
                })
            })
            .collect();
        super::go::link_members(&mut members);
        workspaces.push(Workspace {
            id: id(state, "workspace", &["go", &aggregator]),
            ecosystem: "go",
            aggregator,
            default_members: members.iter().map(|member| member.id.clone()).collect(),
            excluded_paths: Vec::new(),
            members,
        });
    }
}

pub(super) fn go_member(state: &RepositoryState, manifest: String, dir: &str) -> Member {
    let name = read_manifest(state, &manifest)
        .as_deref()
        .and_then(manifests::go_mod_module);
    let member_id = id(state, "member", &["go", &manifest]);
    let (modules, dependencies) = super::go::details(state, dir, name.as_deref(), &member_id);
    let internal_dependencies = dependencies
        .iter()
        .filter(|dependency| dependency.target.is_some())
        .cloned()
        .collect();
    Member {
        id: member_id,
        kind: "go_module",
        name,
        path: dir.to_owned(),
        manifest,
        targets: Vec::new(),
        modules,
        tasks: Vec::new(),
        dependencies,
        internal_dependencies,
    }
}
