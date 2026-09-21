//! JSON rendering for build topology answers.

use super::ManifestIndex;
use super::manifests::CargoManifest;
use super::model::{
    BuildDependency, BuildModel, BuildTarget, BuildTask, Member, Runner, entity_id,
};
use crate::operations::optional_u64;
use blazingly_json::{Value, json};

const MAX_TARGETS: usize = 50;
const MAX_TASKS: usize = 50;
const MAX_RUNNERS: usize = 200;

pub(super) fn report(args: &Value, model: &BuildModel) -> Result<Value, String> {
    let max_members = usize::try_from(optional_u64(args, "max_members")?.unwrap_or(500))
        .unwrap_or(500)
        .clamp(1, 2_000);
    let workspaces_total = model.workspaces.len();
    let members_total = model
        .workspaces
        .iter()
        .map(|workspace| workspace.members.len())
        .sum::<usize>();
    let targets_truncated = model
        .workspaces
        .iter()
        .flat_map(|workspace| &workspace.members)
        .any(|member| member.targets.len() > MAX_TARGETS);
    let tasks_truncated = model
        .workspaces
        .iter()
        .flat_map(|workspace| &workspace.members)
        .any(|member| member.tasks.len() > MAX_TASKS);
    let runners_total = model.runners.len();
    let mut remaining = max_members;
    let rendered = model
        .workspaces
        .iter()
        .map(|workspace| {
            let take = remaining.min(workspace.members.len());
            remaining -= take;
            json!({
                "ecosystem": workspace.ecosystem,
                "aggregator": workspace.aggregator,
                "members_total": workspace.members.len(),
                "members": workspace.members.iter().take(take).map(member).collect::<Vec<_>>()
            })
        })
        .collect::<Vec<_>>();
    Ok(json!({
        "schema_version": model.schema_version,
        "status": if !model.diagnostics.is_empty() || members_total > max_members || targets_truncated || tasks_truncated || runners_total > MAX_RUNNERS {"INCOMPLETE"} else {"COMPLETE"},
        "workspaces": rendered,
        "workspaces_total": workspaces_total,
        "members_total": members_total,
        "members_truncated": members_total > max_members,
        "targets_truncated": targets_truncated,
        "tasks_truncated": tasks_truncated,
        "runners": model.runners.iter().take(MAX_RUNNERS).collect::<Vec<_>>(),
        "runners_total": runners_total,
        "runners_truncated": runners_total > MAX_RUNNERS,
        "manifest_diagnostics": model.diagnostics,
        "model": "manifest and lockfile evidence only; no build tool was executed",
        "semantic_precision": "BOUNDED_STATIC"
    }))
}

fn member(member: &Member) -> Value {
    json!({
        "id": member.id,
        "name": member.name,
        "path": member.path,
        "manifest": member.manifest,
        "targets": member.targets.iter().take(MAX_TARGETS).collect::<Vec<_>>(),
        "targets_total": member.targets.len(),
        "targets_truncated": member.targets.len() > MAX_TARGETS,
        "tasks": member.tasks.iter().take(MAX_TASKS).collect::<Vec<_>>(),
        "tasks_total": member.tasks.len(),
        "tasks_truncated": member.tasks.len() > MAX_TASKS,
        "internal_dependencies": member.internal_dependencies
    })
}

pub(super) fn script_tasks(manifest: &str, scripts: &[(String, String)]) -> Vec<BuildTask> {
    scripts
        .iter()
        .map(|(name, command)| BuildTask {
            id: entity_id("task", &["npm", manifest, name]),
            kind: "script",
            name: name.clone(),
            command: command.clone(),
        })
        .collect()
}

pub(super) fn cargo_targets(
    parsed: &CargoManifest,
    index: &ManifestIndex,
    dir: &str,
    manifest: &str,
) -> Vec<BuildTarget> {
    let mut targets = parsed
        .targets
        .iter()
        .map(|target| {
            build_target(
                manifest,
                target.kind,
                target.name.clone(),
                target.path.clone(),
                false,
                target.required_features.clone(),
            )
        })
        .collect::<Vec<_>>();
    let source = |suffix: &str| {
        if dir.is_empty() {
            suffix.to_owned()
        } else {
            format!("{dir}/{suffix}")
        }
    };
    let mut add_implicit = |kind: &'static str, name: &str, path: &str| {
        if !index.contains_file(&source(path))
            || parsed.targets.iter().any(|target| {
                target.kind == kind
                    && (target.path.as_deref() == Some(path)
                        || (target.name.as_deref() == Some(name)
                            || (kind == "lib" && target.name.is_none())))
            })
        {
            return;
        }
        targets.push(build_target(
            manifest,
            kind,
            Some(name.to_owned()),
            Some(path.to_owned()),
            true,
            Vec::new(),
        ));
    };
    let package_name = parsed.name.as_deref().unwrap_or_default();
    if parsed.autobins != Some(false) {
        add_implicit("bin", package_name, "src/main.rs");
    }
    if parsed.autolib != Some(false) {
        add_implicit("lib", &package_name.replace('-', "_"), "src/lib.rs");
    }
    for (kind, directory, enabled) in [
        ("test", "tests", parsed.autotests != Some(false)),
        ("bench", "benches", parsed.autobenches != Some(false)),
        ("example", "examples", parsed.autoexamples != Some(false)),
    ] {
        if !enabled {
            continue;
        }
        let prefix = source(&format!("{directory}/"));
        for path in index
            .labels()
            .iter()
            .filter_map(|file| file.strip_prefix(&prefix))
        {
            let name =
                if let Some(name) = path.strip_suffix(".rs").filter(|name| !name.contains('/')) {
                    name
                } else if let Some(name) = path
                    .strip_suffix("/main.rs")
                    .filter(|name| !name.contains('/'))
                {
                    name
                } else {
                    continue;
                };
            add_implicit(kind, name, &format!("{directory}/{path}"));
        }
    }
    targets
}

fn build_target(
    manifest: &str,
    kind: &'static str,
    name: Option<String>,
    path: Option<String>,
    implicit: bool,
    required_features: Vec<String>,
) -> BuildTarget {
    let logical = name.as_deref().or(path.as_deref()).unwrap_or(kind);
    BuildTarget {
        id: entity_id("target", &["cargo", manifest, kind, logical]),
        kind,
        name,
        path,
        implicit,
        applicability: if required_features.is_empty() {
            "DECLARED"
        } else {
            "CONDITIONAL_FEATURES"
        },
        required_features,
    }
}

pub(super) fn pending_dependency(name: &str, scope: &'static str) -> BuildDependency {
    BuildDependency {
        id: entity_id("build-dependency", &[name, scope]),
        name: name.to_owned(),
        member: None,
        scope,
        condition: "UNCONDITIONAL_DECLARATION",
    }
}

pub(super) fn path_dependency(
    name: &str,
    member_dir: &str,
    scope: &'static str,
) -> BuildDependency {
    BuildDependency {
        id: entity_id("build-dependency", &[name, member_dir, scope]),
        name: name.to_owned(),
        member: Some(entity_id(
            "member",
            &[
                "cargo",
                &if member_dir.is_empty() {
                    "Cargo.toml".to_owned()
                } else {
                    format!("{member_dir}/Cargo.toml")
                },
            ],
        )),
        scope,
        condition: "UNCONDITIONAL_DECLARATION",
    }
}

pub(super) fn runner_configs(index: &ManifestIndex) -> Vec<Runner> {
    index
        .labels()
        .iter()
        .filter_map(|path| {
            runner_kind(path).map(|kind| Runner {
                id: entity_id("runner", &[kind, path]),
                path: path.clone(),
                kind,
            })
        })
        .collect()
}

fn runner_kind(path: &str) -> Option<&'static str> {
    let normalized = path.to_ascii_lowercase();
    let file = normalized.rsplit('/').next().unwrap_or(normalized.as_str());
    let extension = std::path::Path::new(file)
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or_default();
    if normalized.contains(".github/workflows/") && matches!(extension, "yml" | "yaml") {
        return Some("github-actions");
    }
    let prefixed = [
        ("jest.config.", "jest"),
        ("vitest.config.", "vitest"),
        ("playwright.config.", "playwright"),
        ("cypress.config.", "cypress"),
        ("karma.conf", "karma"),
        (".mocharc", "mocha"),
        ("webpack.config.", "webpack"),
        ("vite.config.", "vite"),
        ("rollup.config.", "rollup"),
        ("babel.config.", "babel"),
        (".babelrc", "babel"),
    ];
    for (prefix, kind) in prefixed {
        if file.starts_with(prefix) {
            return Some(kind);
        }
    }
    if file.starts_with("tsconfig") && extension == "json" {
        return Some("typescript");
    }
    match file {
        "turbo.json" => Some("turbo"),
        "nx.json" => Some("nx"),
        "lerna.json" => Some("lerna"),
        "pnpm-workspace.yaml" => Some("pnpm-workspace"),
        "go.work" => Some("go-work"),
        "makefile" | "gnumakefile" => Some("make"),
        "justfile" => Some("just"),
        "taskfile.yml" | "taskfile.yaml" => Some("task"),
        "pom.xml" => Some("maven"),
        "build.gradle" | "build.gradle.kts" | "settings.gradle" | "settings.gradle.kts" => {
            Some("gradle")
        }
        _ => None,
    }
}
