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
    let targets_truncated = workspaces
        .iter()
        .flat_map(|workspace| &workspace.members)
        .any(|member| member.targets.len() > MAX_TARGETS);
    let runners_total = index
        .labels()
        .iter()
        .filter(|path| runner_kind(path).is_some())
        .count();
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
                "members": workspace.members.iter().take(take).map(member).collect::<Vec<_>>()
            })
        })
        .collect::<Vec<_>>();
    Ok(json!({
        "status": if members_total > max_members || targets_truncated || runners_total > MAX_RUNNERS {"INCOMPLETE"} else {"COMPLETE"},
        "workspaces": rendered,
        "workspaces_total": workspaces_total,
        "members_total": members_total,
        "members_truncated": members_total > max_members,
        "targets_truncated": targets_truncated,
        "runners": runner_configs(index),
        "runners_total": runners_total,
        "runners_truncated": runners_total > MAX_RUNNERS,
        "model": "manifest and lockfile evidence only; no build tool was executed",
        "semantic_precision": "BOUNDED_STATIC"
    }))
}

fn member(member: &Member) -> Value {
    json!({
        "name": member.name,
        "path": member.dir,
        "manifest": member.manifest,
        "targets": member.targets.iter().take(MAX_TARGETS).collect::<Vec<_>>(),
        "targets_total": member.targets.len(),
        "targets_truncated": member.targets.len() > MAX_TARGETS,
        "internal_dependencies": member.internal
    })
}

pub(super) fn script_targets(scripts: &[(String, String)]) -> Vec<Value> {
    scripts
        .iter()
        .map(|(name, command)| json!({"kind": "script", "entity_kind": "task", "name": name, "command": command}))
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
        .map(|target| json!({"kind": target.kind, "name": target.name, "path": target.path,
            "required_features": target.required_features,
            "applicability": if target.required_features.is_empty() {"DECLARED"} else {"CONDITIONAL_FEATURES"},
            "implicit": false}))
        .collect::<Vec<_>>();
    let source = |suffix: &str| {
        if dir.is_empty() {
            suffix.to_owned()
        } else {
            format!("{dir}/{suffix}")
        }
    };
    let mut add_implicit = |kind: &str, name: &str, path: &str| {
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
        targets.push(
            json!({"kind": kind, "name": name, "path": path, "implicit": true,
            "required_features": [], "applicability": "DECLARED"}),
        );
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
        .filter_map(|path| runner_kind(path).map(|kind| json!({"path": path, "kind": kind})))
        .take(MAX_RUNNERS)
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
