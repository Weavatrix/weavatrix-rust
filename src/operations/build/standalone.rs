//! Manifests no aggregator claimed become single-member workspaces.

use super::model::{Workspace, entity_id};
use super::{ManifestIndex, ecosystems, locate, manifests, parent_dir, read_manifest};
use crate::engine::RepositoryState;
use std::collections::BTreeSet;

pub(super) fn packages(
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
        let member = ecosystems::npm_member(state, &manifest, &dir);
        workspaces.push(workspace(state, "npm", manifest, member));
    }
    for manifest in locate(state, index, "Cargo.toml") {
        if claimed.contains(&format!("cargo:{manifest}")) {
            continue;
        }
        let Some(text) = read_manifest(state, &manifest) else {
            continue;
        };
        let parsed = manifests::cargo_manifest(&text);
        if parsed.workspace || parsed.name.is_none() {
            continue;
        }
        let dir = parent_dir(&manifest);
        let member = ecosystems::cargo_member(state, index, &manifest, &dir);
        workspaces.push(workspace(state, "cargo", manifest, member));
    }
    for manifest in locate(state, index, "go.mod") {
        if claimed.contains(&format!("go:{manifest}")) {
            continue;
        }
        let dir = parent_dir(&manifest);
        let member = ecosystems::go_member(state, manifest.clone(), &dir);
        workspaces.push(workspace(state, "go", manifest, member));
    }
    super::python::distributions(state, index, workspaces);
}

fn workspace(
    state: &RepositoryState,
    ecosystem: &'static str,
    manifest: String,
    member: super::model::Member,
) -> Workspace {
    let default_members = vec![member.id.clone()];
    Workspace {
        id: entity_id(
            &state.snapshot().repository,
            "workspace",
            &[ecosystem, &manifest],
        ),
        ecosystem,
        aggregator: manifest,
        default_members,
        excluded_paths: Vec::new(),
        members: vec![member],
    }
}
