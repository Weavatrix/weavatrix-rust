//! JSON rendering for build topology answers.

use super::manifests::CargoManifest;
use super::{ManifestIndex, Member, Workspace};
use crate::operations::optional_u64;
use blazingly_json::{Value, json};

const MAX_TARGETS: usize = 50;
const MAX_RUNNERS: usize = 200;

pub(super) fn report(
    args: &Value,
    workspaces: Vec<Workspace>,
    index: &ManifestIndex,
) -> Result<Value, String> {
    let max_members = usize::try_from(optional_u64(args, "max_members")?.unwrap_or(500))
        .unwrap_or(500)
        .clamp(1, 2_000);
    let workspaces_total = workspaces.len();
    let members_total = workspaces
        .iter()
        .map(|workspace| workspace.members.len())
        .sum::<usize>();
    let mut remaining = max_members;
    let rendered = workspaces
        .into_iter()
        .map(|workspace| {
            let take = remaining.min(workspace.members.len());
            remaining -= take;
            json!({
                "ecosystem": workspace.ecosystem,
                "aggregator": workspace.aggregator,
                "members_total": workspace.members.len(),
                "members": workspace.members.into_iter().take(take).map(member).collect::<Vec<_>>()
            })
        })
        .collect::<Vec<_>>();
    Ok(json!({
        "status": "COMPLETE",
        "workspaces": rendered,
        "workspaces_total": workspaces_total,
        "members_total": members_total,
        "members_truncated": members_total > max_members,
        "runners": runner_configs(index),
        "model": "manifest and lockfile evidence only; no build tool was executed",
        "semantic_precision": "BOUNDED_STATIC"
    }))
}

fn member(member: Member) -> Value {
    json!({
        "name": member.name,
        "path": member.dir,
        "manifest": member.manifest,
        "targets": member.targets,
        "internal_dependencies": member.internal
    })
}

pub(super) fn script_targets(scripts: &[(String, String)]) -> Vec<Value> {
    scripts
        .iter()
        .take(MAX_TARGETS)
        .map(|(name, command)| json!({"kind": "script", "name": name, "command": command}))
        .collect()
}

pub(super) fn cargo_targets(
    parsed: &CargoManifest,
    index: &ManifestIndex,
    dir: &str,
) -> Vec<Value> {
    let mut targets = parsed
        .targets
        .iter()
        .take(MAX_TARGETS)
        .map(|(kind, name)| json!({"kind": kind, "name": name}))
        .collect::<Vec<_>>();
    let source = |suffix: &str| {
        if dir.is_empty() {
            suffix.to_owned()
        } else {
            format!("{dir}/{suffix}")
        }
    };
    if index.contains_file(&source("src/main.rs")) {
        targets.push(json!({
            "kind": "bin", "name": parsed.name, "path": "src/main.rs", "implicit": true
        }));
    }
    if index.contains_file(&source("src/lib.rs")) {
        targets.push(json!({
            "kind": "lib", "name": parsed.name, "path": "src/lib.rs", "implicit": true
        }));
    }
    targets
}

pub(super) fn pending_dependency(name: &str, scope: &str) -> Value {
    json!({"name": name, "scope": scope})
}

pub(super) fn path_dependency(name: &str, member_dir: &str, scope: &str) -> Value {
    json!({"name": name, "member": member_dir, "scope": scope})
}

pub(super) fn stamp_member(dependency: &mut Value, member_dir: &str) {
    if let Some(object) = dependency.as_object_mut() {
        object.insert("member".to_owned(), json!(member_dir));
    }
}

fn runner_configs(index: &ManifestIndex) -> Vec<Value> {
    index
        .labels()
        .iter()
        .filter_map(|path| {
            runner_kind(path).map(|kind| json!({"path": path, "kind": kind}))
        })
        .take(MAX_RUNNERS)
        .collect()
}

fn runner_kind(path: &str) -> Option<&'static str> {
    let normalized = path.to_ascii_lowercase();
    let file = normalized.rsplit('/').next().unwrap_or(normalized.as_str());
    if normalized.contains(".github/workflows/")
        && (file.ends_with(".yml") || file.ends_with(".yaml"))
    {
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
    if file.starts_with("tsconfig") && file.ends_with(".json") {
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
