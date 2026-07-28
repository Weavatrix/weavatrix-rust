use super::health_manifests::{Declaration, duplicates, normalize, parse};
use crate::RepositoryState;
use blazingly_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};
use weavatrix_graph::NodeKind;

const MANIFESTS: &[&str] = &[
    "Cargo.toml",
    "package.json",
    "go.mod",
    "requirements.txt",
    "pyproject.toml",
];

pub(super) fn report(state: &RepositoryState, max: usize) -> Value {
    let declarations = declarations(state);
    let imports = imports(state);
    let duplicate_groups = duplicates(&declarations);
    let mut findings = Vec::new();

    for (language, packages) in &imports {
        let ecosystem = ecosystem(language);
        for package in packages {
            if builtin(language, package)
                || declarations
                    .iter()
                    .any(|item| matches_declaration(item, ecosystem, package))
            {
                continue;
            }
            findings.push(json!({
                "id": format!("dependency.missing:{language}:{package}"),
                "rule": "dependency.missing_declaration",
                "category": "dependencies",
                "severity": "medium",
                "language": language,
                "package": package,
                "message": "external import has no matching supported manifest declaration"
            }));
        }
    }
    for declaration in &declarations {
        let used = languages(declaration.ecosystem).iter().any(|language| {
            imports.get(*language).is_some_and(|packages| {
                packages
                    .iter()
                    .any(|package| matches_declaration(declaration, declaration.ecosystem, package))
            })
        });
        if !used && !development_scope(&declaration.scope) {
            findings.push(json!({
                "id": format!(
                    "dependency.unused:{}:{}:{}",
                    declaration.ecosystem, declaration.manifest, declaration.name
                ),
                "rule": "dependency.unused_declaration",
                "category": "dependencies",
                "severity": "low",
                "manifest": declaration.manifest,
                "package": declaration.name,
                "message": "manifest declaration has no matching static import evidence",
                "caveat": "plugins, build scripts, reflection and generated imports may be invisible"
            }));
        }
    }
    for items in duplicate_groups.values() {
        let first = items[0];
        findings.push(json!({
            "id": format!(
                "dependency.duplicate:{}:{}:{}",
                first.ecosystem, first.manifest, first.name
            ),
            "rule": "dependency.duplicate_declaration",
            "category": "dependencies",
            "severity": "medium",
            "manifest": first.manifest,
            "package": first.name,
            "scopes": items.iter().map(|item| item.scope.as_str()).collect::<Vec<_>>()
        }));
    }
    findings.sort_by(|left, right| left["id"].as_str().cmp(&right["id"].as_str()));
    json!({
        "status": "PARTIAL",
        "declared": declarations.len(),
        "external_imports": imports.values().map(BTreeSet::len).sum::<usize>(),
        "duplicate_declarations": duplicate_groups.len(),
        "findings_total": findings.len(),
        "findings": findings.into_iter().take(max).collect::<Vec<_>>(),
        "coverage": {
            "cargo": "STATIC_COMPLETE",
            "npm": "STATIC_COMPLETE",
            "go": "STATIC_COMPLETE",
            "python": "PARTIAL",
            "maven_gradle": "NOT_AVAILABLE"
        }
    })
}

fn declarations(state: &RepositoryState) -> Vec<Declaration> {
    let mut result = Vec::new();
    for relative in manifest_paths(state) {
        let absolute = state.root().join(Path::new(&relative));
        let Ok(metadata) = fs::metadata(&absolute) else {
            continue;
        };
        if metadata.len() > 2_000_000 {
            continue;
        }
        let Ok(text) = fs::read_to_string(&absolute) else {
            continue;
        };
        result.extend(parse(&absolute, &relative, &text));
    }
    result.sort_by(|left, right| {
        (&left.manifest, left.ecosystem, &left.name, &left.scope).cmp(&(
            &right.manifest,
            right.ecosystem,
            &right.name,
            &right.scope,
        ))
    });
    result
}

fn manifest_paths(state: &RepositoryState) -> BTreeSet<String> {
    let mut directories = BTreeSet::from([PathBuf::new()]);
    for node in state.graph().nodes() {
        if node.kind != NodeKind::File {
            continue;
        }
        let mut directory = Path::new(&node.label).parent();
        while let Some(value) = directory {
            directories.insert(value.to_path_buf());
            directory = value.parent();
        }
    }
    directories
        .into_iter()
        .flat_map(|directory| {
            MANIFESTS
                .iter()
                .map(move |name| directory.join(name).to_string_lossy().replace('\\', "/"))
        })
        .collect()
}

fn imports(state: &RepositoryState) -> BTreeMap<String, BTreeSet<String>> {
    let mut result = BTreeMap::<String, BTreeSet<String>>::new();
    for node in state.graph().nodes() {
        if node.kind != NodeKind::Package {
            continue;
        }
        let Some(language) = &node.language else {
            continue;
        };
        result
            .entry(language.clone())
            .or_default()
            .insert(node.label.clone());
    }
    result
}

fn matches_declaration(item: &Declaration, ecosystem: &str, package: &str) -> bool {
    if item.ecosystem != ecosystem {
        return false;
    }
    let declared = normalize(ecosystem, &item.name);
    let imported = normalize(ecosystem, package);
    imported == declared || (ecosystem == "go" && imported.starts_with(&format!("{declared}/")))
}

fn ecosystem(language: &str) -> &str {
    match language {
        "rust" => "cargo",
        "javascript" | "typescript" => "npm",
        "go" => "go",
        "python" => "python",
        _ => language,
    }
}

fn languages(ecosystem: &str) -> &'static [&'static str] {
    match ecosystem {
        "cargo" => &["rust"],
        "npm" => &["javascript", "typescript"],
        "go" => &["go"],
        "python" => &["python"],
        _ => &[],
    }
}

fn development_scope(scope: &str) -> bool {
    scope.to_ascii_lowercase().contains("dev")
}

fn builtin(language: &str, package: &str) -> bool {
    match language {
        "rust" => matches!(
            package,
            "std" | "core" | "alloc" | "crate" | "self" | "super"
        ),
        "javascript" | "typescript" => package.starts_with("node:"),
        "go" => !package.contains('.'),
        "python" => matches!(
            package,
            "os" | "sys" | "json" | "time" | "typing" | "pathlib" | "collections" | "asyncio"
        ),
        _ => false,
    }
}
