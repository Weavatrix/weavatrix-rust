//! Build topology from manifest evidence: workspace aggregators, members,
//! targets and runner configurations. No build tool is executed.

mod cargo_manifest;
mod ecosystems;
mod manifests;
mod render;

use crate::engine::RepositoryState;
use blazingly_json::Value;
use std::collections::BTreeSet;
use std::fs;
use std::path::Path;
use weavatrix_graph::NodeKind;

const MAX_MANIFEST_BYTES: u64 = 2_000_000;

pub(super) struct Member {
    name: Option<String>,
    dir: String,
    manifest: String,
    targets: Vec<Value>,
    internal: Vec<Value>,
}

pub(super) struct Workspace {
    ecosystem: &'static str,
    aggregator: String,
    members: Vec<Member>,
}

/// Graph file labels plus every directory they imply. Manifest formats the
/// language roster does not parse (TOML, YAML) may be absent from the node
/// inventory, so manifests are located by directory probe as well.
pub(super) struct ManifestIndex {
    labels: BTreeSet<String>,
    directories: BTreeSet<String>,
}

pub(in crate::operations) fn build_graph(
    state: &RepositoryState,
    args: &Value,
) -> Result<Value, String> {
    let index = index(state);
    let mut workspaces = Vec::new();
    let mut claimed = BTreeSet::new();
    ecosystems::npm_workspaces(state, &index, &mut workspaces, &mut claimed);
    ecosystems::cargo_workspaces(state, &index, &mut workspaces, &mut claimed);
    ecosystems::go_workspaces(state, &index, &mut workspaces, &mut claimed);
    ecosystems::standalone_packages(state, &index, &mut workspaces, &claimed);
    workspaces.sort_by(|left, right| {
        (left.ecosystem, &left.aggregator).cmp(&(right.ecosystem, &right.aggregator))
    });
    let mut report = render::report(args, workspaces, &index)?;
    let diagnostics = manifest_diagnostics(state, &index);
    if let Some(object) = report.as_object_mut() {
        if !diagnostics.is_empty() {
            object.insert("status".to_owned(), blazingly_json::json!("INCOMPLETE"));
        }
        object.insert(
            "manifest_diagnostics".to_owned(),
            blazingly_json::json!(diagnostics),
        );
    }
    super::ci::attach(state, &mut report);
    Ok(report)
}

fn manifest_diagnostics(state: &RepositoryState, index: &ManifestIndex) -> Vec<Value> {
    let mut diagnostics = Vec::new();
    for name in ["Cargo.toml", "package.json", "go.mod"] {
        for path in locate(state, index, name) {
            let Some(text) = read_manifest(state, &path) else {
                diagnostics.push(
                    blazingly_json::json!({"manifest": path, "reason": "unreadable_or_excluded"}),
                );
                continue;
            };
            if name == "package.json"
                && blazingly_json::from_str::<Value>(&text)
                    .ok()
                    .is_none_or(|value| !value.is_object())
            {
                diagnostics.push(
                    blazingly_json::json!({"manifest": path, "reason": "invalid_package_json"}),
                );
            }
        }
    }
    diagnostics
}

fn index(state: &RepositoryState) -> ManifestIndex {
    let labels = state
        .graph()
        .nodes()
        .iter()
        .filter(|node| node.kind == NodeKind::File)
        .map(|node| node.label.replace('\\', "/"))
        .collect::<BTreeSet<_>>();
    let mut directories = BTreeSet::from([String::new()]);
    for label in &labels {
        let mut directory = Path::new(label).parent();
        while let Some(value) = directory {
            directories.insert(value.to_string_lossy().replace('\\', "/"));
            directory = value.parent();
        }
    }
    ManifestIndex {
        labels,
        directories,
    }
}

impl ManifestIndex {
    pub(super) fn contains_file(&self, path: &str) -> bool {
        self.labels.contains(path)
    }

    pub(super) fn labels(&self) -> &BTreeSet<String> {
        &self.labels
    }
}

/// Repository-relative paths of every product manifest with this file name,
/// whether or not the graph ingested its format.
pub(super) fn locate(state: &RepositoryState, index: &ManifestIndex, name: &str) -> Vec<String> {
    let mut found = BTreeSet::new();
    for directory in &index.directories {
        let candidate = if directory.is_empty() {
            name.to_owned()
        } else {
            format!("{directory}/{name}")
        };
        if candidate.contains("node_modules/")
            || crate::operations::health::is_non_product(directory)
        {
            continue;
        }
        if index.labels.contains(&candidate) || state.root().join(&candidate).is_file() {
            found.insert(candidate);
        }
    }
    found.into_iter().collect()
}

pub(super) fn read_manifest(state: &RepositoryState, relative: &str) -> Option<String> {
    let absolute = state.root().join(relative);
    let canonical = absolute.canonicalize().ok()?;
    if !canonical.starts_with(state.root()) {
        return None;
    }
    if fs::metadata(&absolute).ok()?.len() > MAX_MANIFEST_BYTES {
        return None;
    }
    // Editors on Windows save manifests with a UTF-8 BOM; a line parser that
    // sees `\u{feff}[package]` misses every section after it.
    fs::read_to_string(&canonical)
        .ok()
        .map(|text| text.trim_start_matches('\u{feff}').to_owned())
}

pub(super) fn parent_dir(path: &str) -> String {
    path.rsplit_once('/')
        .map_or(String::new(), |(dir, _)| dir.to_owned())
}

pub(super) fn dir_is_member(aggregator_dir: &str, patterns: &[String], dir: &str) -> bool {
    let relative = if aggregator_dir.is_empty() {
        Some(dir)
    } else {
        dir.strip_prefix(&format!("{aggregator_dir}/"))
    };
    relative.is_some_and(|relative| {
        !relative.is_empty()
            && patterns.iter().any(|pattern| {
                !pattern.starts_with('!') && manifests::glob_matches(pattern, relative)
            })
            && !patterns.iter().any(|pattern| {
                pattern
                    .strip_prefix('!')
                    .is_some_and(|excluded| manifests::glob_matches(excluded, relative))
            })
    })
}
