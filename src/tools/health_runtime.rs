//! Runtime-correctness review over local evidence.
//!
//! Every check here reads only repository bytes: no network, no package
//! manager, no execution. Each projection reports what it actually covered so
//! a caller can never mistake "nothing configured" for "nothing wrong".

use crate::RepositoryState;
use blazingly_json::{Value, json};
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

/// A finding identity that survives line shifts: rule, file, and a
/// fingerprint of the offending code rather than its position.
fn finding_id(rule: &str, path: &str, line: &str) -> String {
    let normalized = line.split_whitespace().collect::<Vec<_>>().join(" ");
    let mut hash = 0xcbf2_9ce4_8422_2325_u64;
    for byte in normalized.as_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    format!("{rule}:{path}:{hash:016x}")
}

/// One reviewable source: repository path, language identifier, and body.
pub(super) type Source = (String, String, String);

/// Findings, files scanned, and whether the cap truncated the review.
pub(super) type RuntimeReview = (Vec<Value>, usize, bool);

/// Runs the runtime rule set over arbitrary source text, so the same review
/// applies to the worktree and to an immutable Git baseline.
pub(super) fn runtime_findings(
    sources: impl IntoIterator<Item = Source>,
    max: usize,
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
            if ignored_lines.contains(&(offset + 1)) {
                continue;
            }
            if line.len() > 400 {
                continue;
            }
            for rule in RULES {
                if !rule.languages.is_empty() && !rule.languages.contains(&language.as_str()) {
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

fn runtime_code(path: &str, source: &str) -> String {
    let Some(extension) = Path::new(path).extension().and_then(|value| value.to_str()) else {
        return source.to_owned();
    };
    let Some(language) = weavatrix_parse::Language::from_extension(extension) else {
        return source.to_owned();
    };
    let mut code = source.as_bytes().to_vec();
    for token in weavatrix_parse::tokenize(source, language) {
        if matches!(
            token.kind,
            weavatrix_parse::TokenKind::String
                | weavatrix_parse::TokenKind::Regex
                | weavatrix_parse::TokenKind::LineComment
                | weavatrix_parse::TokenKind::BlockComment
                | weavatrix_parse::TokenKind::Unterminated
        ) {
            code[token.start..token.end].fill(b' ');
        }
    }
    String::from_utf8(code).unwrap_or_else(|_| source.to_owned())
}

fn async_context_lines(path: &str, language: &str, source: &str) -> BTreeSet<usize> {
    if language == "python" {
        return python_async_lines(source);
    }
    let Some(extension) = Path::new(path).extension().and_then(|value| value.to_str()) else {
        return BTreeSet::new();
    };
    let Some(language) = weavatrix_parse::Language::from_extension(extension) else {
        return BTreeSet::new();
    };
    if !matches!(
        language,
        weavatrix_parse::Language::JavaScript
            | weavatrix_parse::Language::TypeScript
            | weavatrix_parse::Language::Rust
    ) {
        return BTreeSet::new();
    }
    brace_async_lines(source, language)
}

fn brace_async_lines(source: &str, language: weavatrix_parse::Language) -> BTreeSet<usize> {
    let tokens = weavatrix_parse::tokenize_lite(source, language);
    let mut lines = BTreeSet::new();
    let mut index = 0_usize;
    while index < tokens.len() {
        if tokens[index].text(source) != "async" {
            index += 1;
            continue;
        }
        lines.insert(tokens[index].line as usize);
        let limit = (index + 128).min(tokens.len());
        let Some(open) = (index + 1..limit)
            .take_while(|candidate| tokens[*candidate].text(source) != ";")
            .find(|candidate| tokens[*candidate].text(source) == "{")
        else {
            index += 1;
            continue;
        };
        let mut depth = 0_usize;
        let mut close = None;
        for (candidate, token) in tokens.iter().enumerate().skip(open) {
            match token.text(source) {
                "{" => depth += 1,
                "}" => {
                    depth = depth.saturating_sub(1);
                    if depth == 0 {
                        close = Some(candidate);
                        break;
                    }
                }
                _ => {}
            }
        }
        let Some(close) = close else {
            index += 1;
            continue;
        };
        lines.extend(tokens[index].line as usize..=tokens[close].line as usize);
        index = close + 1;
    }
    lines
}

fn python_async_lines(source: &str) -> BTreeSet<usize> {
    let mut lines = BTreeSet::new();
    let mut scopes = Vec::<usize>::new();
    for (offset, line) in source.lines().enumerate() {
        let trimmed = line.trim_start();
        if trimmed.is_empty() {
            continue;
        }
        let indent = line.len().saturating_sub(trimmed.len());
        while scopes.last().is_some_and(|scope| indent <= *scope) {
            scopes.pop();
        }
        if !scopes.is_empty() {
            lines.insert(offset + 1);
        }
        if trimmed.starts_with("async def ") {
            lines.insert(offset + 1);
            scopes.push(indent);
        }
    }
    lines
}

/// Lines compiled only under `cfg(test)`. Token positions make brace matching
/// insensitive to braces written inside strings or comments.
pub(super) fn rust_cfg_test_lines(source: &str) -> BTreeSet<usize> {
    let tokens = weavatrix_parse::tokenize_lite(source, weavatrix_parse::Language::Rust);
    let mut ignored = BTreeSet::new();
    let mut index = 0_usize;
    while index + 2 < tokens.len() {
        if tokens[index].text(source) != "#"
            || tokens[index + 1].text(source) != "["
            || tokens[index + 2].text(source) != "cfg"
        {
            index += 1;
            continue;
        }
        let Some(attribute_end) =
            (index + 3..tokens.len()).find(|candidate| tokens[*candidate].text(source) == "]")
        else {
            break;
        };
        if !(index + 3..attribute_end).any(|candidate| tokens[candidate].text(source) == "test") {
            index = attribute_end + 1;
            continue;
        }
        let Some(open) = (attribute_end + 1..tokens.len())
            .find(|candidate| tokens[*candidate].text(source) == "{")
        else {
            break;
        };
        let mut depth = 0_usize;
        let mut close = None;
        for (candidate, token) in tokens.iter().enumerate().skip(open) {
            match token.text(source) {
                "{" => depth += 1,
                "}" => {
                    depth = depth.saturating_sub(1);
                    if depth == 0 {
                        close = Some(candidate);
                        break;
                    }
                }
                _ => {}
            }
        }
        let Some(close) = close else {
            break;
        };
        ignored.extend(tokens[index].line as usize..=tokens[close].line as usize);
        index = close + 1;
    }
    ignored
}

/// Production source text of the analyzed worktree.
pub(super) fn product_sources(state: &RepositoryState) -> Vec<Source> {
    state
        .graph()
        .nodes()
        .iter()
        .filter(|node| node.kind == NodeKind::File)
        .filter_map(|node| {
            let language = node.language.clone()?;
            if super::health::is_non_product(&node.label) {
                return None;
            }
            let text = fs::read_to_string(state.root().join(&node.label)).ok()?;
            Some((node.label.clone(), language, text))
        })
        .collect()
}

/// Bounded runtime-correctness and concurrency review over production source.
pub(super) fn runtime(state: &RepositoryState, max: usize) -> Value {
    let (findings, scanned, truncated) = runtime_findings(product_sources(state), max);
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

#[cfg(test)]
mod tests {
    use super::{finding_id, runtime_findings};

    #[test]
    fn finding_identity_survives_line_shifts_and_reindentation() {
        let original = "    for (const item of items) { await save(item); }";
        let shifted = "        for (const item of items) {  await save(item); }";
        assert_eq!(
            finding_id("runtime.await_in_loop", "src/a.js", original),
            finding_id("runtime.await_in_loop", "src/a.js", shifted),
            "indentation and inner spacing must not change a finding identity"
        );
        assert_ne!(
            finding_id("runtime.await_in_loop", "src/a.js", original),
            finding_id("runtime.await_in_loop", "src/b.js", original),
            "the same code in another file is another finding"
        );
    }

    #[test]
    fn runtime_rules_report_stable_findings_for_moved_code() {
        let body = "const items = [];\nfor (const item of items) { await save(item); }\n";
        let moved = format!("// header\n// header\n{body}");
        let (first, scanned, truncated) = runtime_findings(
            [(
                "src/a.js".to_owned(),
                "javascript".to_owned(),
                body.to_owned(),
            )],
            10,
        );
        let (second, _, _) = runtime_findings(
            [("src/a.js".to_owned(), "javascript".to_owned(), moved)],
            10,
        );
        assert_eq!(scanned, 1);
        assert!(!truncated);
        assert_eq!(first.len(), 1, "await inside a loop is reported once");
        assert_eq!(
            first[0]["id"], second[0]["id"],
            "moving code down the file keeps its identity so debt stays comparable"
        );
        assert_ne!(
            first[0]["line"], second[0]["line"],
            "the reported line still follows the code"
        );
    }

    #[test]
    fn runtime_rules_ignore_rust_code_compiled_only_for_tests() {
        let source = r#"
pub fn production() {
    risky().unwrap();
}

#[cfg(test)]
mod tests {
    const JSON: &str = "{ braces in strings do not close the module }";

    #[test]
    fn smoke() {
        risky().expect("tests may assert");
    }
}
"#;
        let (findings, scanned, truncated) = runtime_findings(
            [(
                "src/lib.rs".to_owned(),
                "rust".to_owned(),
                source.to_owned(),
            )],
            10,
        );

        assert_eq!(scanned, 1);
        assert!(!truncated);
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0]["line"], 3);
    }

    #[test]
    fn blocking_calls_are_reported_only_inside_async_contexts() {
        let rust = r"
fn background_worker() {
    std::thread::sleep(std::time::Duration::from_millis(10));
}

async fn request_handler() {
    std::thread::sleep(std::time::Duration::from_millis(10));
}
";
        let python = r"
def background_worker():
    time.sleep(1)

async def request_handler():
    time.sleep(1)
";
        let javascript = r#"
function backgroundWorker() {
    readFileSync("state.json");
}

async function requestHandler() {
    readFileSync("state.json");
}
"#;
        let (findings, scanned, truncated) = runtime_findings(
            [
                ("src/lib.rs".to_owned(), "rust".to_owned(), rust.to_owned()),
                (
                    "worker.py".to_owned(),
                    "python".to_owned(),
                    python.to_owned(),
                ),
                (
                    "worker.js".to_owned(),
                    "javascript".to_owned(),
                    javascript.to_owned(),
                ),
            ],
            10,
        );
        assert_eq!(scanned, 3);
        assert!(!truncated);
        assert_eq!(findings.len(), 3);
        assert!(
            findings.iter().all(|finding| {
                let expected = if finding["file"] == "worker.py" { 6 } else { 7 };
                finding["line"] == expected
            }),
            "{findings:#?}"
        );
    }
}
