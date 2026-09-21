//! Line-level manifest parsing for build topology: workspace aggregators,
//! member names, targets and path dependencies. Text in, facts out.

use blazingly_json::Value;

use super::ManifestIndex;
use super::model::{Runner, entity_id};

pub(super) struct NpmPackage {
    pub(super) name: Option<String>,
    pub(super) scripts: Vec<(String, String)>,
    pub(super) dependencies: Vec<(String, &'static str)>,
}

pub(super) fn npm_package(text: &str) -> NpmPackage {
    let document = blazingly_json::from_str::<Value>(text).unwrap_or(Value::Null);
    let scripts = document["scripts"]
        .as_object()
        .into_iter()
        .flat_map(|scripts| {
            scripts
                .iter()
                .filter_map(|(name, command)| Some((name.clone(), command.as_str()?.to_owned())))
        })
        .collect();
    let mut dependencies = Vec::new();
    for (key, scope) in [
        ("dependencies", "dependencies"),
        ("devDependencies", "devDependencies"),
        ("peerDependencies", "peerDependencies"),
        ("optionalDependencies", "optionalDependencies"),
    ] {
        for (name, _) in document[key]
            .as_object()
            .into_iter()
            .flat_map(|object| object.iter())
        {
            dependencies.push((name.clone(), scope));
        }
    }
    NpmPackage {
        name: document["name"].as_str().map(str::to_owned),
        scripts,
        dependencies,
    }
}

/// `workspaces` as an array or the `{"packages": [...]}` object form.
pub(super) fn npm_workspace_patterns(text: &str) -> Option<Vec<String>> {
    let document = blazingly_json::from_str::<Value>(text).ok()?;
    let workspaces = document.get("workspaces")?;
    let patterns = workspaces
        .as_array()
        .or_else(|| workspaces["packages"].as_array())?;
    Some(
        patterns
            .iter()
            .filter_map(|pattern| pattern.as_str().map(str::to_owned))
            .collect(),
    )
}

/// `packages` from `pnpm-workspace.yaml`: the list items under the key.
pub(super) fn yaml_packages(text: &str) -> Vec<String> {
    let mut result = Vec::new();
    let mut in_packages = false;
    for line in text.lines() {
        let trimmed = line.split('#').next().unwrap_or_default().trim_end();
        if trimmed.trim() == "packages:" {
            in_packages = true;
            continue;
        }
        if in_packages {
            let item = trimmed.trim_start();
            if let Some(value) = item.strip_prefix("- ") {
                result.push(value.trim().trim_matches(['"', '\'']).to_owned());
            } else if !item.is_empty() && !line.starts_with([' ', '\t']) {
                break;
            }
        }
    }
    result
}

/// `packages` from `lerna.json`.
pub(super) fn json_packages(text: &str) -> Vec<String> {
    blazingly_json::from_str::<Value>(text)
        .ok()
        .and_then(|document| {
            document["packages"].as_array().map(|patterns| {
                patterns
                    .iter()
                    .filter_map(|pattern| pattern.as_str().map(str::to_owned))
                    .collect()
            })
        })
        .unwrap_or_default()
}

pub(super) use super::cargo_manifest::cargo_manifest;

/// `use ( ./a ./b )` and single-line `use ./a` from `go.work`.
pub(super) fn go_work_uses(text: &str) -> Vec<String> {
    let mut result = Vec::new();
    let mut in_block = false;
    for raw in text.lines() {
        let line = raw.split("//").next().unwrap_or_default().trim();
        if in_block {
            if line == ")" {
                in_block = false;
            } else if !line.is_empty() {
                result.push(line.trim_matches(['"']).to_owned());
            }
            continue;
        }
        if let Some(rest) = line.strip_prefix("use") {
            let rest = rest.trim();
            if rest == "(" {
                in_block = true;
            } else if !rest.is_empty() {
                result.push(rest.trim_matches(['"']).to_owned());
            }
        }
    }
    result
}

pub(super) fn go_mod_module(text: &str) -> Option<String> {
    text.lines()
        .filter_map(|line| line.split("//").next())
        .find_map(|line| line.trim().strip_prefix("module "))
        .map(|module| module.trim().to_owned())
}

/// Segment-wise glob: `*` is one segment, `**` any run of segments, anything
/// else is literal. `packages/*` matches `packages/api`, not `packages/api/x`.
pub(super) fn glob_matches(pattern: &str, path: &str) -> bool {
    fn matches(pattern: &[&str], path: &[&str]) -> bool {
        match (pattern.first(), path.first()) {
            (None, None) => true,
            (Some(&"**"), _) => {
                matches(&pattern[1..], path) || (!path.is_empty() && matches(pattern, &path[1..]))
            }
            (Some(&"*"), Some(_)) => matches(&pattern[1..], &path[1..]),
            (Some(literal), Some(segment)) if literal == segment => {
                matches(&pattern[1..], &path[1..])
            }
            _ => false,
        }
    }
    let pattern = pattern
        .trim_start_matches("./")
        .split('/')
        .filter(|segment| !segment.is_empty())
        .collect::<Vec<_>>();
    let path = path
        .split('/')
        .filter(|segment| !segment.is_empty() && *segment != ".")
        .collect::<Vec<_>>();
    matches(&pattern, &path)
}

/// Joins a manifest directory with a relative path, resolving `.` and `..`
/// without touching the filesystem. Escaping the repository returns None.
pub(super) fn normalize_relative(base_dir: &str, relative: &str) -> Option<String> {
    let mut segments = base_dir
        .split('/')
        .filter(|segment| !segment.is_empty())
        .map(str::to_owned)
        .collect::<Vec<_>>();
    for segment in relative.replace('\\', "/").split('/') {
        match segment {
            "" | "." => {}
            ".." => {
                segments.pop()?;
            }
            other => segments.push(other.to_owned()),
        }
    }
    Some(segments.join("/"))
}

pub(super) fn runner_configs(repository: &str, index: &ManifestIndex) -> Vec<Runner> {
    index
        .labels()
        .iter()
        .filter_map(|path| {
            runner_kind(path).map(|kind| Runner {
                id: entity_id(repository, "runner", &[kind, path]),
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
    for (prefix, kind) in [
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
    ] {
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
