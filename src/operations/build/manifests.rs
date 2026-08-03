//! Line-level manifest parsing for build topology: workspace aggregators,
//! member names, targets and path dependencies. Text in, facts out.

use blazingly_json::Value;

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
            scripts.iter().filter_map(|(name, command)| {
                Some((name.clone(), command.as_str()?.to_owned()))
            })
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

#[derive(Default)]
pub(super) struct CargoManifest {
    pub(super) name: Option<String>,
    pub(super) workspace_members: Vec<String>,
    pub(super) workspace_excludes: Vec<String>,
    pub(super) targets: Vec<(&'static str, Option<String>)>,
    pub(super) path_dependencies: Vec<(String, String, &'static str)>,
}

enum CargoSection {
    Package,
    Workspace,
    Dependencies(&'static str),
    Target(usize),
    Other,
}

pub(super) fn cargo_manifest(text: &str) -> CargoManifest {
    let mut manifest = CargoManifest::default();
    let mut section = CargoSection::Other;
    let mut array: Option<(bool, Vec<String>)> = None;
    for raw in text.lines() {
        let line = raw.split('#').next().unwrap_or_default().trim();
        if let Some((for_members, values)) = array.as_mut() {
            values.extend(quoted_strings(line));
            if line.contains(']') {
                let values = std::mem::take(values);
                if *for_members {
                    manifest.workspace_members = values;
                } else {
                    manifest.workspace_excludes = values;
                }
                array = None;
            }
            continue;
        }
        if line.starts_with('[') {
            section = cargo_section(line, &mut manifest);
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        let (key, value) = (key.trim(), value.trim());
        match &section {
            CargoSection::Package if key == "name" => {
                manifest.name = quoted_strings(value).into_iter().next();
            }
            CargoSection::Workspace if key == "members" || key == "exclude" => {
                let values = quoted_strings(value);
                if value.contains(']') {
                    if key == "members" {
                        manifest.workspace_members = values;
                    } else {
                        manifest.workspace_excludes = values;
                    }
                } else {
                    array = Some((key == "members", values));
                }
            }
            CargoSection::Dependencies(scope) => {
                if let Some(path) = inline_path_value(value) {
                    manifest
                        .path_dependencies
                        .push((key.to_owned(), path, scope));
                }
            }
            CargoSection::Target(index) if key == "name" => {
                if let Some(target) = manifest.targets.get_mut(*index) {
                    target.1 = quoted_strings(value).into_iter().next();
                }
            }
            _ => {}
        }
    }
    manifest
}

fn cargo_section(line: &str, manifest: &mut CargoManifest) -> CargoSection {
    match line {
        "[package]" => CargoSection::Package,
        "[workspace]" => CargoSection::Workspace,
        "[dependencies]" => CargoSection::Dependencies("dependencies"),
        "[dev-dependencies]" => CargoSection::Dependencies("dev-dependencies"),
        "[build-dependencies]" => CargoSection::Dependencies("build-dependencies"),
        "[lib]" => {
            manifest.targets.push(("lib", None));
            CargoSection::Target(manifest.targets.len() - 1)
        }
        "[[bin]]" | "[[bench]]" | "[[test]]" | "[[example]]" => {
            let kind = match line {
                "[[bin]]" => "bin",
                "[[bench]]" => "bench",
                "[[test]]" => "test",
                _ => "example",
            };
            manifest.targets.push((kind, None));
            CargoSection::Target(manifest.targets.len() - 1)
        }
        _ => CargoSection::Other,
    }
}

/// `{ path = "../core", version = "1" }` -> `../core`.
fn inline_path_value(value: &str) -> Option<String> {
    let start = value.find("path")?;
    let rest = value[start + 4..].trim_start();
    let rest = rest.strip_prefix('=')?;
    quoted_strings(rest).into_iter().next()
}

fn quoted_strings(text: &str) -> Vec<String> {
    text.split('"')
        .enumerate()
        .filter_map(|(index, part)| (index % 2 == 1).then(|| part.to_owned()))
        .collect()
}

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
                matches(&pattern[1..], path)
                    || (!path.is_empty() && matches(pattern, &path[1..]))
            }
            (Some(&"*"), Some(_)) => matches(&pattern[1..], &path[1..]),
            (Some(literal), Some(segment)) if literal.eq_ignore_ascii_case(segment) => {
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
