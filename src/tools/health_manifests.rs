use serde_json::Value;
use std::collections::BTreeMap;
use std::path::Path;

#[derive(Debug, Clone)]
pub(super) struct Declaration {
    pub ecosystem: &'static str,
    pub name: String,
    pub manifest: String,
    pub scope: String,
}

pub(super) fn parse(path: &Path, relative: &str, text: &str) -> Vec<Declaration> {
    match path.file_name().and_then(|name| name.to_str()) {
        Some("package.json") => package_json(relative, text),
        Some("Cargo.toml") => sectioned(relative, text, "cargo", &["dependencies"]),
        Some("go.mod") => go_mod(relative, text),
        Some("requirements.txt") => requirements(relative, text),
        Some("pyproject.toml") => pyproject(relative, text),
        _ => Vec::new(),
    }
}

fn package_json(relative: &str, text: &str) -> Vec<Declaration> {
    let Ok(document) = serde_json::from_str::<Value>(text) else {
        return Vec::new();
    };
    let mut result = Vec::new();
    for scope in [
        "dependencies",
        "devDependencies",
        "peerDependencies",
        "optionalDependencies",
    ] {
        let Some(entries) = document.get(scope).and_then(Value::as_object) else {
            continue;
        };
        result.extend(entries.keys().map(|name| Declaration {
            ecosystem: "npm",
            name: name.clone(),
            manifest: relative.to_owned(),
            scope: scope.to_owned(),
        }));
    }
    result
}

fn sectioned(
    relative: &str,
    text: &str,
    ecosystem: &'static str,
    sections: &[&str],
) -> Vec<Declaration> {
    let mut result = Vec::new();
    let mut section = String::new();
    for raw in text.lines() {
        let line = raw.split('#').next().unwrap_or_default().trim();
        if line.starts_with('[') && line.ends_with(']') {
            section = line.trim_matches(['[', ']']).to_ascii_lowercase();
            continue;
        }
        if !sections
            .iter()
            .any(|name| section == *name || section.ends_with(&format!(".{name}")))
        {
            continue;
        }
        let Some((name, _)) = line.split_once('=') else {
            continue;
        };
        let name = name.trim().trim_matches(['"', '\'']);
        if !name.is_empty() {
            result.push(Declaration {
                ecosystem,
                name: name.to_owned(),
                manifest: relative.to_owned(),
                scope: section.clone(),
            });
        }
    }
    result
}

fn go_mod(relative: &str, text: &str) -> Vec<Declaration> {
    let mut result = Vec::new();
    let mut in_require = false;
    for raw in text.lines() {
        let line = raw.split("//").next().unwrap_or_default().trim();
        if line == "require (" {
            in_require = true;
            continue;
        }
        if in_require && line == ")" {
            in_require = false;
            continue;
        }
        let value = line
            .strip_prefix("require ")
            .or_else(|| in_require.then_some(line));
        let Some(value) = value else {
            continue;
        };
        let name = value.split_whitespace().next().unwrap_or_default();
        if !name.is_empty() {
            result.push(Declaration {
                ecosystem: "go",
                name: name.to_owned(),
                manifest: relative.to_owned(),
                scope: "require".to_owned(),
            });
        }
    }
    result
}

fn requirements(relative: &str, text: &str) -> Vec<Declaration> {
    text.lines()
        .filter_map(|raw| {
            let line = raw.split('#').next()?.trim();
            if line.is_empty() || line.starts_with('-') {
                return None;
            }
            let name = line
                .split(['=', '<', '>', '!', '~', '[', ';'])
                .next()
                .unwrap_or_default()
                .trim();
            (!name.is_empty()).then(|| Declaration {
                ecosystem: "python",
                name: name.to_owned(),
                manifest: relative.to_owned(),
                scope: "requirements".to_owned(),
            })
        })
        .collect()
}

fn pyproject(relative: &str, text: &str) -> Vec<Declaration> {
    let mut result = sectioned(
        relative,
        text,
        "python",
        &["project.dependencies", "tool.poetry.dependencies"],
    );
    result.retain(|item| item.name != "python");
    result
}

pub(super) fn duplicates(declarations: &[Declaration]) -> BTreeMap<String, Vec<&Declaration>> {
    let mut grouped = BTreeMap::<String, Vec<&Declaration>>::new();
    for declaration in declarations {
        grouped
            .entry(format!(
                "{}:{}:{}",
                declaration.manifest,
                declaration.ecosystem,
                normalize(declaration.ecosystem, &declaration.name)
            ))
            .or_default()
            .push(declaration);
    }
    grouped.retain(|_, items| items.len() > 1);
    grouped
}

pub(super) fn normalize(ecosystem: &str, name: &str) -> String {
    let normalized = name.trim().to_ascii_lowercase();
    if matches!(ecosystem, "cargo" | "python") {
        normalized.replace('-', "_")
    } else {
        normalized
    }
}
