use crate::engine::RepositoryState;
use std::collections::BTreeSet;
use std::fs;
use std::path::Path;
use weavatrix_graph::NodeKind;

/// Whether an import names a library target produced by this repository.
///
/// Cargo binary and example targets legitimately import their sibling library
/// by package name. That is a project edge, not a missing external dependency.
pub(super) fn contains(state: &RepositoryState, ecosystem: &str, package: &str) -> bool {
    ecosystem == "cargo"
        && cargo_names(state).contains(&package.trim().to_ascii_lowercase().replace('-', "_"))
}

fn cargo_names(state: &RepositoryState) -> BTreeSet<String> {
    state
        .graph()
        .nodes()
        .iter()
        .filter(|node| {
            node.kind == NodeKind::File
                && Path::new(&node.label)
                    .file_name()
                    .is_some_and(|name| name.eq_ignore_ascii_case("Cargo.toml"))
        })
        .filter_map(|node| fs::read_to_string(state.root().join(&node.label)).ok())
        .filter_map(|manifest| cargo_package_name(&manifest))
        .map(|name| name.to_ascii_lowercase().replace('-', "_"))
        .collect()
}

fn cargo_package_name(manifest: &str) -> Option<String> {
    let mut package_section = false;
    for raw in manifest.lines() {
        let line = raw.split('#').next().unwrap_or_default().trim();
        if line.starts_with('[') && line.ends_with(']') {
            package_section = line.trim_matches(['[', ']']) == "package";
            continue;
        }
        if !package_section {
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        if key.trim() == "name" {
            return Some(value.trim().trim_matches(['"', '\'']).to_owned());
        }
    }
    None
}
