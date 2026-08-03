use super::{
    audit::severity_at_least,
    manifests::{Declaration, duplicates, normalize, parse},
    paths::is_non_product,
    project_identity,
};
use crate::engine::RepositoryState;
use blazingly_json::{Value, json};
use import_evidence::{ImportEvidence, installed_peer_obligations};
use lexicon::{builtin, development_scope, ecosystem, languages, matches_declaration};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};
use weavatrix_graph::NodeKind;

#[path = "dependency_imports.rs"]
mod import_evidence;
#[path = "dependency_lexicon.rs"]
mod lexicon;

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
    let imports = ImportEvidence::collect(state);
    let direct_consumers = declarations
        .iter()
        .filter(|declaration| declaration_has_import(declaration, &imports))
        .collect::<Vec<_>>();
    let peer_obligations = installed_peer_obligations(state.root(), &direct_consumers);
    let peer_required = peer_obligations
        .iter()
        .map(|obligation| {
            (
                obligation.manifest.clone(),
                normalize("npm", &obligation.package),
            )
        })
        .collect::<BTreeSet<_>>();
    let duplicate_groups = duplicates(&declarations);
    let mut findings = missing_declaration_findings(state, &declarations, &imports);
    findings.extend(unused_declaration_findings(
        &declarations,
        &imports,
        &peer_required,
    ));
    findings.extend(duplicate_findings(&duplicate_groups));
    filter_findings(&mut findings, enabled, min_severity);
    findings.sort_by(|left, right| left["id"].as_str().cmp(&right["id"].as_str()));
    json!({
        "status": if findings.is_empty() {"PASS"} else {"REVIEW"},
        "execution": {"status": "COMPLETE"},
        "declared": declarations.len(),
        "external_imports": imports.external.values().map(BTreeSet::len).sum::<usize>(),
        "peer_obligations": peer_obligations.iter().map(|obligation| json!({
            "manifest": obligation.manifest,
            "consumer": obligation.consumer,
            "package": obligation.package,
            "evidence": obligation.evidence,
            "required": true
        })).collect::<Vec<_>>(),
        "duplicate_declarations": duplicate_groups.len(),
        "findings_total": findings.len(),
        "findings": findings.into_iter().take(max).collect::<Vec<_>>(),
        "manifest_evidence": manifest_evidence(&declarations)
    })
}

fn missing_declaration_findings(
    state: &RepositoryState,
    declarations: &[Declaration],
    imports: &ImportEvidence,
) -> Vec<Value> {
    let mut findings = Vec::new();
    for (language, packages) in &imports.external {
        let ecosystem = ecosystem(language);
        for package in packages {
            if builtin(language, package)
                || project_identity::contains(state, ecosystem, package)
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
    findings
}

fn unused_declaration_findings(
    declarations: &[Declaration],
    imports: &ImportEvidence,
    peer_required: &BTreeSet<(String, String)>,
) -> Vec<Value> {
    let mut findings = Vec::new();
    for declaration in declarations {
        let used = declaration_has_import(declaration, imports)
            || (declaration.ecosystem == "npm"
                && peer_required.contains(&(
                    declaration.manifest.clone(),
                    normalize("npm", &declaration.name),
                )));
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
    findings
}

fn duplicate_findings(duplicate_groups: &BTreeMap<String, Vec<&Declaration>>) -> Vec<Value> {
    duplicate_groups
        .values()
        .map(|items| {
            let first = items[0];
            json!({
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
            })
        })
        .collect()
}

fn manifest_evidence(declarations: &[Declaration]) -> Value {
    json!({
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
    })
}

fn declaration_has_import(declaration: &Declaration, imports: &ImportEvidence) -> bool {
    languages(declaration.ecosystem).iter().any(|language| {
        imports.all.get(*language).is_some_and(|packages| {
            packages
                .iter()
                .any(|package| matches_declaration(declaration, declaration.ecosystem, package))
        })
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
