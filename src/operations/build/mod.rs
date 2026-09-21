//! Build topology from manifest evidence: workspace aggregators, members,
//! targets and runner configurations. No build tool is executed.

mod cargo_manifest;
mod conditions;
mod ecosystem_details;
mod ecosystems;
mod go;
mod manifests;
mod model;
mod python;
mod render;

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
    attach_completeness(&model, &mut report);
    super::ci::attach(state, &mut report);
    Ok(report)
}

pub(crate) fn architecture_topology(model: &BuildModel) -> Value {
    let unknowns = semantic_unknowns(model);
    json!({
        "schema_version": model.schema_version,
        "status": if model.diagnostics.is_empty() && model.input_capture.complete && unknowns == 0 {"COMPLETE"} else {"INCOMPLETE"},
        "analysis": {"coverage": if unknowns == 0 {"BOUNDED_STATIC"} else {"BOUNDED_STATIC_WITH_UNRESOLVED"},
                     "unknowns_total": unknowns, "closed_world": false},
        "workspaces": &model.workspaces,
        "runners": &model.runners,
        "diagnostics": &model.diagnostics,
        "input_capture": model.input_capture,
        "semantic_precision": "BOUNDED_STATIC"
    })
}

fn attach_completeness(model: &BuildModel, report: &mut Value) {
    let unknowns = semantic_unknowns(model);
    report["analysis"] = json!({
        "coverage": if unknowns == 0 {"BOUNDED_STATIC"} else {"BOUNDED_STATIC_WITH_UNRESOLVED"},
        "unknowns_total": unknowns,
        "closed_world": false
    });
    if unknowns > 0 {
        report["status"] = json!("INCOMPLETE");
    }
}

fn semantic_unknowns(model: &BuildModel) -> usize {
    model
        .workspaces
        .iter()
        .flat_map(|workspace| &workspace.members)
        .flat_map(|member| &member.dependencies)
        .filter(|dependency| dependency.resolution.contains("UNRESOLVED"))
        .count()
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
    standalone_packages(state, &index, &mut workspaces, &claimed);
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
    for name in ["Cargo.toml", "package.json", "go.mod", "pyproject.toml"] {
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
            } else if name == "Cargo.toml" && !manifests::cargo_manifest(&text).valid {
                diagnostics.push(BuildDiagnostic {
                    manifest: path,
                    reason: "invalid_cargo_manifest",
                });
            } else if name == "pyproject.toml" && !python::valid_manifest(&text) {
                diagnostics.push(BuildDiagnostic {
                    manifest: path,
                    reason: "invalid_pyproject_manifest",
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

fn standalone_packages(
    state: &RepositoryState,
    index: &ManifestIndex,
    workspaces: &mut Vec<model::Workspace>,
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
        workspaces.push(single_member_workspace(state, "npm", manifest, member));
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
        let member = ecosystems::cargo_member(state, index, &manifest, &dir, None);
        workspaces.push(single_member_workspace(state, "cargo", manifest, member));
    }
    for manifest in locate(state, index, "go.mod") {
        if claimed.contains(&format!("go:{manifest}")) {
            continue;
        }
        let dir = parent_dir(&manifest);
        let member = ecosystems::go_member(state, manifest.clone(), &dir);
        workspaces.push(single_member_workspace(state, "go", manifest, member));
    }
    python::distributions(state, index, workspaces);
}

fn single_member_workspace(
    state: &RepositoryState,
    ecosystem: &'static str,
    manifest: String,
    member: model::Member,
) -> model::Workspace {
    let default_members = vec![member.id.clone()];
    model::Workspace {
        id: model::entity_id(
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
