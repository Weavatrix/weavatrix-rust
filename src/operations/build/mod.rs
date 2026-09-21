//! Build topology from manifest evidence: workspace aggregators, members,
//! targets and runner configurations. No build tool is executed.

mod cargo_manifest;
mod conditions;
mod ecosystem_details;
mod ecosystems;
mod manifests;
mod model;
mod python;
mod render;
mod standalone;

use crate::engine::RepositoryState;
use blazingly_json::{Value, json};
pub(crate) use model::BuildModel;
use model::{BuildDiagnostic, InputCapture};
use std::collections::BTreeSet;
use std::path::Path;
use weavatrix_graph::NodeKind;

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
    let (model, _) = analyze(state);
    let mut report = render::report(args, &model)?;
    super::ci::attach(state, &mut report);
    Ok(report)
}

pub(crate) fn architecture_topology(model: &BuildModel) -> Value {
    json!({
        "schema_version": model.schema_version,
        "status": if model.diagnostics.is_empty() && model.input_capture.complete {"COMPLETE"} else {"INCOMPLETE"},
        "workspaces": &model.workspaces,
        "runners": &model.runners,
        "diagnostics": &model.diagnostics,
        "input_capture": model.input_capture,
        "semantic_precision": "BOUNDED_STATIC"
    })
}

pub(crate) fn model(state: &RepositoryState) -> BuildModel {
    analyze(state).0
}

fn analyze(state: &RepositoryState) -> (BuildModel, ManifestIndex) {
    let index = index(state);
    let mut workspaces = Vec::new();
    let mut claimed = BTreeSet::new();
    ecosystems::npm_workspaces(state, &index, &mut workspaces, &mut claimed);
    ecosystems::cargo_workspaces(state, &index, &mut workspaces, &mut claimed);
    ecosystems::go_workspaces(state, &index, &mut workspaces, &mut claimed);
    standalone::packages(state, &index, &mut workspaces, &claimed);
    workspaces.sort_by(|left, right| {
        (left.ecosystem, &left.aggregator).cmp(&(right.ecosystem, &right.aggregator))
    });
    let diagnostics = manifest_diagnostics(state, &index);
    let runners = manifests::runner_configs(&state.snapshot().repository, &index);
    let model = BuildModel {
        schema_version: "weavatrix.build-model.v1",
        repository: state.snapshot().repository.clone(),
        revision: state.snapshot().revision.clone(),
        workspaces,
        runners,
        diagnostics,
        input_capture: InputCapture {
            generation: state.evidence().generation().to_owned(),
            complete: state.evidence().receipt().complete,
            excluded: state.evidence().receipt().excluded.to_vec(),
        },
    };
    (model, index)
}

fn manifest_diagnostics(state: &RepositoryState, index: &ManifestIndex) -> Vec<BuildDiagnostic> {
    let mut diagnostics = Vec::new();
    for name in ["Cargo.toml", "package.json", "go.mod"] {
        for path in locate(state, index, name) {
            let Some(text) = read_manifest(state, &path) else {
                diagnostics.push(BuildDiagnostic {
                    manifest: path,
                    reason: "unreadable_or_excluded",
                });
                continue;
            };
            if name == "package.json"
                && blazingly_json::from_str::<Value>(&text)
                    .ok()
                    .is_none_or(|value| !value.is_object())
            {
                diagnostics.push(BuildDiagnostic {
                    manifest: path,
                    reason: "invalid_package_json",
                });
            }
        }
    }
    diagnostics
}

fn index(state: &RepositoryState) -> ManifestIndex {
    let mut labels = state
        .graph()
        .nodes()
        .iter()
        .filter(|node| node.kind == NodeKind::File)
        .map(|node| node.label.replace('\\', "/"))
        .collect::<BTreeSet<_>>();
    labels.extend(state.evidence().paths().map(str::to_owned));
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
pub(super) fn locate(_state: &RepositoryState, index: &ManifestIndex, name: &str) -> Vec<String> {
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
        if index.labels.contains(&candidate) {
            found.insert(candidate);
        }
    }
    found.into_iter().collect()
}

pub(super) fn read_manifest(state: &RepositoryState, relative: &str) -> Option<String> {
    // Editors on Windows save manifests with a UTF-8 BOM; a line parser that
    // sees `\u{feff}[package]` misses every section after it.
    state
        .evidence()
        .text(relative)
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
