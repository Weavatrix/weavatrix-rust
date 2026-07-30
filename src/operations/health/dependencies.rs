use super::{
    audit::severity_at_least,
    manifests::{Declaration, duplicates, normalize, parse},
    paths::is_non_product,
    runtime::rust_cfg_test_lines,
};
use crate::engine::RepositoryState;
use blazingly_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::fs;
use std::path::{Path, PathBuf};
use weavatrix_graph::{Edge, EdgeKind, Node, NodeKind};

const MANIFESTS: &[&str] = &[
    "Cargo.toml",
    "package.json",
    "go.mod",
    "requirements.txt",
    "pyproject.toml",
];

pub(super) fn report(state: &RepositoryState, max: usize, filter: (bool, u8)) -> Value {
    let (enabled, min_severity) = filter;
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
    filter_findings(&mut findings, enabled, min_severity);
    findings.sort_by(|left, right| left["id"].as_str().cmp(&right["id"].as_str()));
    json!({
        "status": if findings.is_empty() {"PASS"} else {"REVIEW"},
        "execution": {"status": "COMPLETE"},
        "declared": declarations.len(),
        "external_imports": imports.values().map(BTreeSet::len).sum::<usize>(),
        "duplicate_declarations": duplicate_groups.len(),
        "findings_total": findings.len(),
        "findings": findings.into_iter().take(max).collect::<Vec<_>>(),
        "manifest_evidence": {
            "present": !declarations.is_empty(),
            "reason": if declarations.is_empty() {
                json!("no Cargo.toml, package.json, go.mod, requirements.txt, or pyproject.toml dependency declarations were found")
            } else {
                Value::Null
            },
            "formats": {
                "cargo": {"present": true, "scope": "Cargo.toml dependency sections"},
                "npm": {"present": true, "scope": "package.json dependency sections"},
                "go": {"present": true, "scope": "go.mod require directives"},
                "python": {"present": true, "scope": "requirements.txt and pyproject.toml dependency declarations"},
                "maven_gradle": {
                    "present": false,
                    "reason": "Maven and Gradle manifests are not inputs to this dependency audit contract"
                }
            }
        }
    })
}

fn filter_findings(findings: &mut Vec<Value>, enabled: bool, min_severity: u8) {
    findings.retain(|finding| {
        enabled
            && finding["severity"]
                .as_str()
                .is_some_and(|severity| severity_at_least(severity, min_severity))
    });
}

fn declarations(state: &RepositoryState) -> Vec<Declaration> {
    let mut result = Vec::new();
    for relative in manifest_paths(state) {
        if manifest_is_non_product(&relative) {
            continue;
        }
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

fn manifest_is_non_product(relative: &str) -> bool {
    Path::new(relative)
        .parent()
        .is_some_and(|parent| is_non_product(&parent.to_string_lossy()))
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
    let graph = state.graph();
    let mut rust_test_lines = HashMap::<String, BTreeSet<usize>>::new();
    for edge in graph.edges() {
        if edge.kind != EdgeKind::Imports {
            continue;
        }
        let (Some(source), Some(package)) = (
            graph.node(edge.source.as_str()),
            graph.node(edge.target.as_str()),
        ) else {
            continue;
        };
        if package.kind != NodeKind::Package
            || !production_import(state, source, edge, &mut rust_test_lines)
        {
            continue;
        }
        let Some(language) = &package.language else {
            continue;
        };
        result
            .entry(language.clone())
            .or_default()
            .insert(package.label.clone());
    }
    result
}

fn production_import(
    state: &RepositoryState,
    source: &Node,
    edge: &Edge,
    rust_test_lines: &mut HashMap<String, BTreeSet<usize>>,
) -> bool {
    if source.kind != NodeKind::File || is_non_product(&source.label) {
        return false;
    }
    if source.language.as_deref() != Some("rust") {
        return true;
    }
    let Some(span) = edge.provenance.span.as_ref() else {
        return true;
    };
    let lines = rust_test_lines
        .entry(source.label.clone())
        .or_insert_with(|| {
            fs::read_to_string(state.root().join(&source.label))
                .map_or_else(|_| BTreeSet::new(), |text| rust_cfg_test_lines(&text))
        });
    !lines.contains(&usize::try_from(span.start.line).unwrap_or(usize::MAX))
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
