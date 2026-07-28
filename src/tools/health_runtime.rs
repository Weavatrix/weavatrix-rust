//! Runtime-correctness, advisory and malware review over local evidence.
//!
//! Every check here reads only repository bytes: no network, no package
//! manager, no execution. Each projection reports what it actually covered so
//! a caller can never mistake "nothing configured" for "nothing wrong".

use crate::RepositoryState;
use serde_json::{Value, json};
use std::collections::BTreeSet;
use std::fs;
use std::path::Path;
use weavatrix_graph::NodeKind;

/// One bounded runtime-correctness pattern.
struct Rule {
    id: &'static str,
    severity: &'static str,
    languages: &'static [&'static str],
    message: &'static str,
    /// Line-level trigger; the second argument is the enclosing block text.
    matches: fn(&str) -> bool,
}

const RULES: &[Rule] = &[
    Rule {
        id: "runtime.await_in_loop",
        severity: "medium",
        languages: &["javascript", "typescript", "python"],
        message: "await inside a loop serializes iterations; consider batching",
        matches: |line| {
            let trimmed = line.trim_start();
            (trimmed.starts_with("for ") || trimmed.starts_with("while "))
                && line.contains("await ")
        },
    },
    Rule {
        id: "runtime.floating_promise",
        severity: "high",
        languages: &["javascript", "typescript"],
        message: "promise-returning call is neither awaited nor chained; rejections are unobserved",
        matches: |line| {
            let trimmed = line.trim();
            trimmed.ends_with(");")
                && (trimmed.starts_with("fetch(")
                    || trimmed.contains(".then(")
                    || trimmed.starts_with("Promise.all("))
                && !trimmed.contains("await ")
                && !trimmed.contains(".catch(")
                && !trimmed.starts_with("return ")
        },
    },
    Rule {
        id: "runtime.empty_catch",
        severity: "high",
        languages: &[],
        message: "error is swallowed by an empty catch/except block",
        matches: |line| {
            let trimmed = line.trim().replace(' ', "");
            trimmed == "catch{}"
                || trimmed.ends_with("catch{}")
                || trimmed == "except:pass"
                || trimmed.ends_with("=>{}),")
        },
    },
    Rule {
        id: "runtime.blocking_call_in_async",
        severity: "high",
        languages: &["javascript", "typescript", "python", "rust"],
        message: "blocking or sleeping call on an async path stalls the executor",
        matches: |line| {
            line.contains("readFileSync")
                || line.contains("execSync")
                || line.contains("time.sleep(")
                || line.contains("std::thread::sleep")
        },
    },
    Rule {
        id: "runtime.unchecked_unwrap",
        severity: "medium",
        languages: &["rust"],
        message: "unwrap/expect on fallible values panics in production paths",
        matches: |line| {
            (line.contains(".unwrap()") || line.contains(".expect(")) && !line.contains("//")
        },
    },
    Rule {
        id: "runtime.shared_mutable_global",
        severity: "medium",
        languages: &["javascript", "typescript", "python", "go"],
        message: "mutable module-level state is shared across concurrent requests",
        matches: |line| {
            let trimmed = line.trim_start();
            line.starts_with(|c: char| !c.is_whitespace())
                && (trimmed.starts_with("let cache")
                    || trimmed.starts_with("var cache")
                    || trimmed.starts_with("let current")
                    || trimmed.starts_with("global "))
        },
    },
];

/// Bounded runtime-correctness and concurrency review over production source.
pub(super) fn runtime(state: &RepositoryState, max: usize) -> Value {
    let mut findings = Vec::new();
    let mut scanned = 0_usize;
    let mut truncated = false;
    for node in state.graph().nodes() {
        if node.kind != NodeKind::File {
            continue;
        }
        let Some(language) = node.language.as_deref() else {
            continue;
        };
        if super::health::is_non_product(&node.label) {
            continue;
        }
        let Ok(text) = fs::read_to_string(state.root().join(&node.label)) else {
            continue;
        };
        scanned += 1;
        for (offset, line) in text.lines().enumerate() {
            if line.len() > 400 {
                continue;
            }
            for rule in RULES {
                if !rule.languages.is_empty() && !rule.languages.contains(&language) {
                    continue;
                }
                if (rule.matches)(line) {
                    if findings.len() >= max {
                        truncated = true;
                        break;
                    }
                    findings.push(json!({
                        "id": format!("{}:{}:{}", rule.id, node.label, offset + 1),
                        "rule": rule.id,
                        "category": "runtime",
                        "severity": rule.severity,
                        "file": node.label,
                        "line": offset + 1,
                        "language": language,
                        "message": rule.message,
                        "evidence": line.trim(),
                    }));
                }
            }
        }
    }
    findings.sort_by(|left, right| left["id"].as_str().cmp(&right["id"].as_str()));
    json!({
        "status": if findings.is_empty() {"PASS"} else {"REVIEW"},
        "completeness": "PARTIAL_PATTERN_BASED",
        "rules": RULES.iter().map(|rule| rule.id).collect::<Vec<_>>(),
        "files_scanned": scanned,
        "findings_total": findings.len(),
        "truncated": truncated,
        "findings": findings,
        "caveat": "line-level patterns over production source; no data-flow or execution",
    })
}

/// Offline dependency-risk advisories: version-pinning and lockfile evidence,
/// plus any local advisory database the repository ships.
pub(super) fn advisories(state: &RepositoryState, max: usize) -> Value {
    let mut findings = Vec::new();
    let mut manifests = Vec::new();
    let mut lockfiles = Vec::new();
    for (manifest, locks) in [
        ("Cargo.toml", &["Cargo.lock"][..]),
        (
            "package.json",
            &[
                "package-lock.json",
                "npm-shrinkwrap.json",
                "yarn.lock",
                "pnpm-lock.yaml",
            ],
        ),
        ("go.mod", &["go.sum"]),
        (
            "pyproject.toml",
            &["poetry.lock", "uv.lock", "requirements.txt"],
        ),
    ] {
        if !state.root().join(manifest).is_file() {
            continue;
        }
        manifests.push(manifest);
        let present = locks
            .iter()
            .filter(|lock| state.root().join(lock).is_file())
            .collect::<Vec<_>>();
        if present.is_empty() {
            findings.push(json!({
                "id": format!("advisory.unlocked:{manifest}"),
                "rule": "advisory.missing_lockfile",
                "category": "advisories",
                "severity": "high",
                "manifest": manifest,
                "message": "declared dependencies have no committed lockfile; installed versions are not reproducible",
            }));
        } else {
            for lock in present {
                lockfiles.push(*lock);
            }
        }
        findings.extend(floating_ranges(state.root(), manifest));
    }
    let local_database = advisory_database(state.root());
    findings.sort_by(|left, right| left["id"].as_str().cmp(&right["id"].as_str()));
    let total = findings.len();
    json!({
        "status": if findings.is_empty() {"PASS"} else {"REVIEW"},
        "completeness": if local_database.is_some() {
            "PARTIAL_LOCAL_DATABASE"
        } else {
            "PARTIAL_OFFLINE_NO_CVE_FEED"
        },
        "covers": [
            "missing lockfiles",
            "floating version ranges",
            "advisory identifiers referenced by a committed local database",
        ],
        "does_not_cover": "published CVE/RUSTSEC/OSV feeds; this engine never opens a network connection",
        "manifests": manifests,
        "lockfiles": lockfiles,
        "local_advisory_database": local_database,
        "findings_total": total,
        "findings": findings.into_iter().take(max).collect::<Vec<_>>(),
    })
}

/// Version requirements that accept arbitrary future releases.
fn floating_ranges(root: &Path, manifest: &str) -> Vec<Value> {
    let Ok(text) = fs::read_to_string(root.join(manifest)) else {
        return Vec::new();
    };
    let mut findings = Vec::new();
    for (offset, line) in text.lines().enumerate() {
        let trimmed = line.trim();
        if trimmed.starts_with('#') || trimmed.starts_with("//") {
            continue;
        }
        let floating = trimmed.contains("\"*\"")
            || trimmed.contains("= \"*\"")
            || trimmed.contains("\"latest\"")
            || trimmed.contains(">=")
                && !trimmed.contains(", <")
                && !trimmed.contains(",<")
                && manifest != "go.mod";
        if !floating {
            continue;
        }
        let name = trimmed
            .split(['=', ':'])
            .next()
            .unwrap_or(trimmed)
            .trim()
            .trim_matches('"');
        findings.push(json!({
            "id": format!("advisory.floating:{manifest}:{name}"),
            "rule": "advisory.floating_version_range",
            "category": "advisories",
            "severity": "medium",
            "manifest": manifest,
            "package": name,
            "line": offset + 1,
            "message": "unbounded version requirement accepts arbitrary future releases",
        }));
    }
    findings
}

fn advisory_database(root: &Path) -> Option<String> {
    for candidate in [
        ".weavatrix/advisories.json",
        "advisories.json",
        "security/advisories.json",
    ] {
        if root.join(candidate).is_file() {
            return Some(candidate.to_owned());
        }
    }
    None
}

/// Malware heuristics over installed third-party packages.
pub(super) fn malware(state: &RepositoryState, max: usize, requested: bool) -> Value {
    let roots = ["node_modules", "vendor", ".venv/Lib/site-packages"]
        .into_iter()
        .filter(|directory| state.root().join(directory).is_dir())
        .collect::<Vec<_>>();
    if !requested {
        return json!({
            "status": "NOT_REQUESTED",
            "completeness": "AVAILABLE_ON_REQUEST",
            "installed_trees": roots,
            "message": "pass include_malware_scan:true to grep installed packages; the scan reads many files and is off by default",
        });
    }
    if roots.is_empty() {
        return json!({
            "status": "PASS",
            "completeness": "COMPLETE_NO_INSTALLED_TREES",
            "message": "no installed third-party package tree exists in this checkout, so there is nothing to scan",
        });
    }
    let mut findings = Vec::new();
    let mut scanned = 0_usize;
    let mut packages = BTreeSet::new();
    for directory in &roots {
        scan_tree(
            &state.root().join(directory),
            state.root(),
            0,
            &mut scanned,
            &mut packages,
            &mut findings,
            max,
        );
    }
    findings.sort_by(|left, right| left["id"].as_str().cmp(&right["id"].as_str()));
    json!({
        "status": if findings.is_empty() {"PASS"} else {"REVIEW"},
        "completeness": "PARTIAL_HEURISTIC",
        "installed_trees": roots,
        "packages_seen": packages.len(),
        "files_scanned": scanned,
        "findings_total": findings.len(),
        "findings": findings.into_iter().take(max).collect::<Vec<_>>(),
        "caveat": "heuristic signals, not a verdict; obfuscated or novel payloads can evade them",
    })
}

const MALWARE_SIGNALS: &[(&str, &str, &str)] = &[
    ("malware.install_hook_network", "high", "curl "),
    ("malware.install_hook_network", "high", "wget "),
    ("malware.remote_eval", "critical", "eval(Buffer.from("),
    ("malware.remote_eval", "critical", "child_process.exec("),
    ("malware.credential_probe", "high", "process.env.NPM_TOKEN"),
    ("malware.credential_probe", "high", "id_rsa"),
    ("malware.obfuscated_payload", "medium", "base64,eval"),
];

#[allow(clippy::too_many_arguments)]
fn scan_tree(
    directory: &Path,
    root: &Path,
    depth: usize,
    scanned: &mut usize,
    packages: &mut BTreeSet<String>,
    findings: &mut Vec<Value>,
    max: usize,
) {
    if depth > 6 || findings.len() >= max || *scanned > 20_000 {
        return;
    }
    let Ok(entries) = fs::read_dir(directory) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            scan_tree(&path, root, depth + 1, scanned, packages, findings, max);
            continue;
        }
        let name = path
            .file_name()
            .and_then(|value| value.to_str())
            .unwrap_or_default();
        let extension = Path::new(name)
            .extension()
            .and_then(|value| value.to_str())
            .map(str::to_ascii_lowercase)
            .unwrap_or_default();
        let interesting =
            name == "package.json" || matches!(extension.as_str(), "js" | "cjs" | "sh" | "py");
        if !interesting {
            continue;
        }
        if fs::metadata(&path).is_ok_and(|meta| meta.len() > 512_000) {
            continue;
        }
        let Ok(text) = fs::read_to_string(&path) else {
            continue;
        };
        *scanned += 1;
        let relative = path
            .strip_prefix(root)
            .unwrap_or(&path)
            .to_string_lossy()
            .replace('\\', "/");
        if name == "package.json" {
            packages.insert(relative.clone());
            if text.contains("\"postinstall\"")
                || text.contains("\"preinstall\"")
                || text.contains("\"install\"")
            {
                findings.push(json!({
                    "id": format!("malware.install_script:{relative}"),
                    "rule": "malware.install_script",
                    "category": "malware",
                    "severity": "medium",
                    "file": relative,
                    "message": "installed package declares an install lifecycle script that runs on npm install",
                }));
            }
        }
        for (rule, severity, needle) in MALWARE_SIGNALS {
            if text.contains(needle) {
                findings.push(json!({
                    "id": format!("{rule}:{relative}:{needle}"),
                    "rule": rule,
                    "category": "malware",
                    "severity": severity,
                    "file": relative,
                    "signal": needle,
                    "message": "installed package contains a signal associated with supply-chain payloads",
                }));
            }
            if findings.len() >= max {
                return;
            }
        }
    }
}
