//! Per-ecosystem workspace discovery: npm, Cargo and Go.

use super::manifests::{cargo_manifest, normalize_relative, npm_package};
use super::{
    ManifestIndex, Member, Workspace, dir_is_member, locate, manifests, parent_dir, read_manifest,
    render,
};
use crate::engine::RepositoryState;
use std::collections::{BTreeMap, BTreeSet};

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
        link_npm_members(&mut members);
        workspaces.push(Workspace {
            ecosystem: "npm",
            aggregator,
            members,
        });
    }
}

fn npm_member(state: &RepositoryState, manifest: &str, dir: &str) -> Member {
    let package = read_manifest(state, manifest)
        .as_deref()
        .map_or_else(|| npm_package(""), npm_package);
    Member {
        name: package.name,
        dir: dir.to_owned(),
        manifest: manifest.to_owned(),
        targets: render::script_targets(&package.scripts),
        internal: package
            .dependencies
            .iter()
            .map(|(name, scope)| render::pending_dependency(name, scope))
            .collect(),
    }
}

/// Keeps only dependencies whose name is another member of the same
/// workspace, and stamps that member's directory on the edge.
fn link_npm_members(members: &mut [Member]) {
    let names = members
        .iter()
        .filter_map(|member| Some((member.name.clone()?, member.dir.clone())))
        .collect::<BTreeMap<_, _>>();
    for member in members.iter_mut() {
        member.internal.retain_mut(|dependency| {
            let name = dependency["name"].as_str().unwrap_or_default();
            names.get(name).is_some_and(|dir| {
                render::stamp_member(dependency, dir);
                true
            })
        });
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
        if parsed.workspace_members.is_empty() {
            continue;
        }
        let aggregator_dir = parent_dir(&aggregator);
        let mut members = Vec::new();
        for manifest in locate(state, index, "Cargo.toml") {
            let dir = parent_dir(&manifest);
            if !dir_is_member(&aggregator_dir, &parsed.workspace_members, &dir)
                || dir_is_member(&aggregator_dir, &parsed.workspace_excludes, &dir)
            {
                continue;
            }
            claimed.insert(format!("cargo:{manifest}"));
            members.push(cargo_member(state, index, &manifest, &dir));
        }
        let dirs = members
            .iter()
            .map(|member| member.dir.clone())
            .collect::<BTreeSet<_>>();
        for member in &mut members {
            member.internal.retain(|dependency| {
                dependency["member"]
                    .as_str()
                    .is_some_and(|dir| dirs.contains(dir))
            });
        }
        workspaces.push(Workspace {
            ecosystem: "cargo",
            aggregator,
            members,
        });
    }
}

fn cargo_member(
    state: &RepositoryState,
    index: &ManifestIndex,
    manifest: &str,
    dir: &str,
) -> Member {
    let parsed = read_manifest(state, manifest)
        .as_deref()
        .map(cargo_manifest)
        .unwrap_or_default();
    let internal = parsed
        .path_dependencies
        .iter()
        .filter_map(|(name, path, scope)| {
            let target = normalize_relative(dir, path)?;
            Some(render::path_dependency(name, &target, scope))
        })
        .collect();
    Member {
        name: parsed.name.clone(),
        dir: dir.to_owned(),
        manifest: manifest.to_owned(),
        targets: render::cargo_targets(&parsed, index, dir),
        internal,
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
        let members = manifests::go_work_uses(&text)
            .iter()
            .filter_map(|used| {
                let dir = normalize_relative(&aggregator_dir, used)?;
                let manifest = if dir.is_empty() {
                    "go.mod".to_owned()
                } else {
                    format!("{dir}/go.mod")
                };
                let exists =
                    index.contains_file(&manifest) || state.root().join(&manifest).is_file();
                exists.then(|| {
                    claimed.insert(format!("go:{manifest}"));
                    let name = read_manifest(state, &manifest)
                        .as_deref()
                        .and_then(manifests::go_mod_module);
                    Member {
                        name,
                        dir,
                        manifest,
                        targets: Vec::new(),
                        internal: Vec::new(),
                    }
                })
            })
            .collect();
        workspaces.push(Workspace {
            ecosystem: "go",
            aggregator,
            members,
        });
    }
}

/// Manifests no aggregator claimed become single-member entries, so the
/// answer still covers repositories without any workspace file.
pub(super) fn standalone_packages(
    state: &RepositoryState,
    index: &ManifestIndex,
    workspaces: &mut Vec<Workspace>,
    claimed: &BTreeSet<String>,
) {
    for manifest in locate(state, index, "package.json") {
        let key = format!("npm:{manifest}");
        let is_aggregator = workspaces
            .iter()
            .any(|workspace| workspace.aggregator == manifest);
        if claimed.contains(&key) || is_aggregator {
            continue;
        }
        let dir = parent_dir(&manifest);
        let member = npm_member(state, &manifest, &dir);
        workspaces.push(Workspace {
            ecosystem: "npm",
            aggregator: manifest,
            members: vec![member],
        });
    }
    for manifest in locate(state, index, "Cargo.toml") {
        if claimed.contains(&format!("cargo:{manifest}")) {
            continue;
        }
        let Some(text) = read_manifest(state, &manifest) else {
            continue;
        };
        let parsed = cargo_manifest(&text);
        if !parsed.workspace_members.is_empty() || parsed.name.is_none() {
            continue;
        }
        let dir = parent_dir(&manifest);
        let member = cargo_member(state, index, &manifest, &dir);
        workspaces.push(Workspace {
            ecosystem: "cargo",
            aggregator: manifest,
            members: vec![member],
        });
    }
    for manifest in locate(state, index, "go.mod") {
        if claimed.contains(&format!("go:{manifest}")) {
            continue;
        }
        let name = read_manifest(state, &manifest)
            .as_deref()
            .and_then(manifests::go_mod_module);
        let dir = parent_dir(&manifest);
        workspaces.push(Workspace {
            ecosystem: "go",
            aggregator: manifest.clone(),
            members: vec![Member {
                name,
                dir,
                manifest,
                targets: Vec::new(),
                internal: Vec::new(),
            }],
        });
    }
}
