use crate::model::captured::CaptureExclusion;
use serde::Serialize;
use std::collections::BTreeMap;

pub(super) use super::conditions::{BuildCondition, TargetOptions};

#[derive(Clone, Debug, Serialize)]
pub(crate) struct BuildModel {
    pub schema_version: &'static str,
    pub repository: String,
    pub revision: String,
    pub workspaces: Vec<Workspace>,
    pub runners: Vec<Runner>,
    pub diagnostics: Vec<BuildDiagnostic>,
    pub input_capture: InputCapture,
}

#[derive(Clone, Debug, Serialize)]
pub(crate) struct InputCapture {
    pub generation: String,
    pub complete: bool,
    pub excluded: Vec<CaptureExclusion>,
}

#[derive(Clone, Debug, Serialize)]
pub(crate) struct Workspace {
    pub id: String,
    pub ecosystem: &'static str,
    pub aggregator: String,
    pub default_members: Vec<String>,
    pub excluded_paths: Vec<String>,
    pub members: Vec<Member>,
}

#[derive(Clone, Debug, Serialize)]
pub(crate) struct Member {
    pub id: String,
    pub kind: &'static str,
    pub name: Option<String>,
    pub path: String,
    pub manifest: String,
    pub targets: Vec<BuildTarget>,
    pub modules: Vec<SourceModule>,
    pub tasks: Vec<BuildTask>,
    pub internal_dependencies: Vec<BuildDependency>,
}

#[derive(Clone, Debug, Serialize)]
pub(crate) struct BuildTarget {
    pub id: String,
    pub kind: &'static str,
    pub name: Option<String>,
    pub path: Option<String>,
    pub implicit: bool,
    pub required_features: Vec<String>,
    pub source_patterns: Vec<String>,
    pub conditions: Vec<String>,
    pub condition_ast: BuildCondition,
    pub options: TargetOptions,
    pub applicability: &'static str,
}

#[derive(Clone, Debug, Serialize)]
pub(crate) struct SourceModule {
    pub id: String,
    pub kind: &'static str,
    pub name: String,
    pub path: String,
    pub source_files: Vec<String>,
    pub conditions: Vec<String>,
    pub source_conditions: BTreeMap<String, Vec<String>>,
    pub condition_ast: BuildCondition,
    pub source_condition_ast: BTreeMap<String, BuildCondition>,
    pub applicability: &'static str,
}

#[derive(Clone, Debug, Serialize)]
pub(crate) struct BuildTask {
    pub id: String,
    pub kind: &'static str,
    pub name: String,
    pub command: String,
}

#[derive(Clone, Debug, Serialize)]
pub(crate) struct BuildDependency {
    pub id: String,
    pub name: String,
    pub member: Option<String>,
    pub source_target: Option<String>,
    pub target: Option<String>,
    pub scope: &'static str,
    pub condition: &'static str,
    pub condition_ast: BuildCondition,
}

#[derive(Clone, Debug, Serialize)]
pub(crate) struct Runner {
    pub id: String,
    pub path: String,
    pub kind: &'static str,
}

#[derive(Clone, Debug, Serialize)]
pub(crate) struct BuildDiagnostic {
    pub manifest: String,
    pub reason: &'static str,
}

#[derive(Clone, Debug, Serialize)]
pub(crate) struct FileMembership {
    pub file: String,
    pub member_ids: Vec<String>,
    pub target_ids: Vec<String>,
    pub module_ids: Vec<String>,
    pub resolution: &'static str,
}

impl BuildModel {
    pub fn file_membership(&self, file: &str) -> FileMembership {
        let mut member_ids = Vec::new();
        let mut target_ids = Vec::new();
        let mut module_ids = Vec::new();
        let mut exact_target = false;
        for workspace in &self.workspaces {
            for member in &workspace.members {
                if !contains(&member.path, file) {
                    continue;
                }
                member_ids.push(member.id.clone());
                let relative = relative_to(&member.path, file);
                for target in &member.targets {
                    if target.path.as_deref().is_some_and(|path| {
                        let absolute = join(&member.path, path);
                        file == absolute || owns_source(target.kind, path, relative)
                    }) {
                        exact_target |= target
                            .path
                            .as_deref()
                            .is_some_and(|path| file == join(&member.path, path));
                        target_ids.push(target.id.clone());
                    }
                }
                module_ids.extend(
                    member
                        .modules
                        .iter()
                        .filter(|module| module.source_files.iter().any(|source| source == file))
                        .map(|module| module.id.clone()),
                );
            }
        }
        member_ids.sort();
        member_ids.dedup();
        target_ids.sort();
        target_ids.dedup();
        module_ids.sort();
        module_ids.dedup();
        let resolution = if exact_target {
            "EXACT_TARGET_PATH"
        } else if !target_ids.is_empty() {
            "TARGET_SOURCE_FALLBACK"
        } else if !file.is_empty() {
            "MEMBER_PATH_FALLBACK"
        } else {
            "UNMATCHED"
        };
        FileMembership {
            file: file.to_owned(),
            member_ids,
            target_ids,
            module_ids,
            resolution,
        }
    }

    pub fn task_ids(&self, name: &str, member_ids: &[String]) -> Vec<String> {
        let mut ids = self
            .workspaces
            .iter()
            .flat_map(|workspace| &workspace.members)
            .filter(|member| member_ids.contains(&member.id))
            .flat_map(|member| &member.tasks)
            .filter(|task| task.name == name)
            .map(|task| task.id.clone())
            .collect::<Vec<_>>();
        ids.sort();
        ids.dedup();
        ids
    }

    pub fn target_ids(&self, kind: &str, name: Option<&str>, member_ids: &[String]) -> Vec<String> {
        let mut ids = self
            .workspaces
            .iter()
            .flat_map(|workspace| &workspace.members)
            .filter(|member| member_ids.contains(&member.id))
            .flat_map(|member| &member.targets)
            .filter(|target| {
                target.kind == kind && name.is_none_or(|name| target.name.as_deref() == Some(name))
            })
            .map(|target| target.id.clone())
            .collect::<Vec<_>>();
        ids.sort();
        ids.dedup();
        ids
    }

    pub fn target_exists(&self, kind: &str, name: &str, working: Option<&str>) -> bool {
        let working = working
            .unwrap_or_default()
            .trim_start_matches("./")
            .trim_end_matches('/');
        self.workspaces
            .iter()
            .flat_map(|workspace| &workspace.members)
            .filter(|member| member.path == working)
            .flat_map(|member| &member.targets)
            .any(|target| target.kind == kind && target.name.as_deref() == Some(name))
    }
}

pub(super) fn entity_id(repository: &str, kind: &str, parts: &[&str]) -> String {
    let mut id = format!("{kind}:");
    for (index, part) in std::iter::once(&repository).chain(parts.iter()).enumerate() {
        if index > 0 {
            id.push('.');
        }
        for byte in part.as_bytes() {
            use std::fmt::Write;
            write!(&mut id, "{byte:02x}").expect("writing into String");
        }
    }
    id
}

fn contains(root: &str, file: &str) -> bool {
    root.is_empty() || file == root || file.starts_with(&format!("{root}/"))
}

fn join(root: &str, path: &str) -> String {
    if root.is_empty() {
        path.to_owned()
    } else {
        format!("{root}/{path}")
    }
}

fn relative_to<'a>(root: &str, file: &'a str) -> &'a str {
    if root.is_empty() {
        file
    } else {
        file.strip_prefix(&format!("{root}/")).unwrap_or(file)
    }
}

fn owns_source(kind: &str, target_path: &str, file: &str) -> bool {
    match kind {
        "lib" | "bin" => target_path.starts_with("src/") && file.starts_with("src/"),
        _ => false,
    }
}
