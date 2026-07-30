//! Runtime-correctness review over local repository evidence.

mod rules;
mod source;
#[cfg(test)]
mod tests;

use crate::engine::RepositoryState;
use crate::operations::health::audit::severity_at_least;
use blazingly_json::{Value, json};
use rules::{RULES, finding_id};
use source::{async_context_lines, runtime_code};
use std::collections::BTreeSet;

pub(in crate::operations::health) use source::{product_sources, rust_cfg_test_lines};

/// One reviewable source: repository path, language identifier, and body.
pub(in crate::operations) type Source = (String, String, String);

/// Findings, files scanned, and whether the cap truncated the review.
pub(super) type RuntimeReview = (Vec<Value>, usize, bool);

/// Runs the rule set over arbitrary source text for worktree and Git baselines.
#[cfg(any(feature = "git", test))]
pub(in crate::operations::health) fn runtime_findings(
    sources: impl IntoIterator<Item = Source>,
    max: usize,
) -> RuntimeReview {
    runtime_findings_with_minimum(sources, max, 0)
}

fn runtime_findings_with_minimum(
    sources: impl IntoIterator<Item = Source>,
    max: usize,
    min_severity: u8,
) -> RuntimeReview {
    let mut findings = Vec::new();
    let mut scanned = 0_usize;
    let mut truncated = false;
    for (path, language, text) in sources {
        scanned += 1;
        let ignored_lines = if language == "rust" {
            rust_cfg_test_lines(&text)
        } else {
            BTreeSet::new()
        };
        let code = runtime_code(&path, &text);
        let async_lines = async_context_lines(&path, &language, &code);
        for (offset, (line, code_line)) in text.lines().zip(code.lines()).enumerate() {
            if ignored_lines.contains(&(offset + 1)) || line.len() > 400 {
                continue;
            }
            for rule in RULES {
                if !rule.languages.is_empty() && !rule.languages.contains(&language.as_str()) {
                    continue;
                }
                if !severity_at_least(rule.severity, min_severity) {
                    continue;
                }
                if rule.id == "runtime.blocking_call_in_async"
                    && !async_lines.contains(&(offset + 1))
                {
                    continue;
                }
                if (rule.matches)(code_line) {
                    if findings.len() >= max {
                        truncated = true;
                        break;
                    }
                    findings.push(json!({
                        "id": finding_id(rule.id, &path, line),
                        "rule": rule.id,
                        "category": "runtime",
                        "severity": rule.severity,
                        "file": path,
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
    (findings, scanned, truncated)
}

/// Bounded runtime-correctness review over production source.
pub(super) fn runtime(
    state: &RepositoryState,
    max: usize,
    enabled: bool,
    min_severity: u8,
) -> Value {
    let threshold = if enabled { min_severity } else { u8::MAX };
    let (findings, scanned, truncated) =
        runtime_findings_with_minimum(product_sources(state), max, threshold);
    json!({
        "status": if findings.is_empty() {"PASS"} else {"REVIEW"},
        "execution": {"status": "COMPLETE"},
        "static_analysis": {
            "present": true,
            "scope": "bounded line-level rules over production source"
        },
        "runtime_evidence": {
            "present": false,
            "reason": "run_audit does not execute the repository or ingest profiler and telemetry data"
        },
        "rules": RULES.iter().map(|rule| rule.id).collect::<Vec<_>>(),
        "files_scanned": scanned,
        "findings_total": findings.len(),
        "truncated": truncated,
        "findings": findings,
        "caveat": "bounded syntax-context patterns over production source; no data-flow or execution",
    })
}
