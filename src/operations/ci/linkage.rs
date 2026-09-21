//! Static linkage from local checks to declared architecture rules. A source
//! mention alone is not a proven invocation or observed execution.

use crate::engine::RepositoryState;
use crate::operations::build::BuildModel;
use blazingly_json::{Value, json};

pub(super) fn declared_rule_bindings(
    state: &RepositoryState,
    build: &BuildModel,
    contract: &Value,
    selected: &[Value],
    checks: &Value,
) -> (Vec<Value>, usize) {
    let Some(rules) = contract.get("dependencyRules").and_then(Value::as_array) else {
        return (Vec::new(), 0);
    };
    let checker = checks
        .as_array()
        .into_iter()
        .flatten()
        .find_map(|check| checker_link(state, build, check));
    let matching = rules
        .iter()
        .filter(|rule| {
            rule["from"].as_array().is_some_and(|sources| {
                sources.iter().any(|source| {
                    selected.iter().any(|file| {
                        file["declared_components"]
                            .as_array()
                            .is_some_and(|ids| ids.contains(source))
                    })
                })
            })
        })
        .collect::<Vec<_>>();
    let total = matching.len();
    (
        matching
            .into_iter()
            .take(50)
            .map(|rule| {
                json!({
                    "rule_id": rule["id"],
                    "declaration": ".weavatrix/architecture.json",
                    "checker": checker.as_ref().map(|checker| &checker.id),
                    "linkage": checker.as_ref().map_or("UNBOUND", |checker| checker.linkage),
                    "failure_effect": checker.as_ref().map(|checker| &checker.failure_effect),
                    "execution": "NOT_OBSERVED",
                    "merge_requirement": "NOT_OBSERVED"
                })
            })
            .collect(),
        total,
    )
}

struct Checker {
    id: String,
    linkage: &'static str,
    failure_effect: Value,
}

fn checker_link(state: &RepositoryState, build: &BuildModel, check: &Value) -> Option<Checker> {
    if check["kind"] == "architecture_verification" {
        return Some(checker(check, "LITERAL_VERIFIER_INVOCATION"));
    }
    if check["kind"] != "cargo_test" || check["target_resolved"] != true {
        return None;
    }
    let target = check["target"].as_str()?;
    let working = check["working_directory"].as_str();
    let package = check["package"].as_str();
    let manifest = check["manifest_path"].as_str();
    build
        .target_sources("test", target, working, package, manifest)
        .into_iter()
        .any(|source| {
            state
                .evidence()
                .text(&source)
                .is_some_and(has_failure_aware_verifier_call)
        })
        .then(|| checker(check, "STATIC_FAILURE_PATH_CANDIDATE"))
}

fn checker(check: &Value, linkage: &'static str) -> Checker {
    Checker {
        id: check["id"].as_str().unwrap_or_default().to_owned(),
        linkage,
        failure_effect: check["failure_effect"].clone(),
    }
}

fn has_failure_aware_verifier_call(text: &str) -> bool {
    let code = without_comments(text);
    code.lines().any(|line| {
        let line = line.trim();
        line.contains("verify_architecture(")
            && (line.contains("assert!(")
                || line.contains("assert_eq!(")
                || line.contains(".unwrap()")
                || line.contains(".expect(")
                || line.ends_with('?'))
    })
}

fn without_comments(text: &str) -> String {
    let mut output = String::with_capacity(text.len());
    let mut block = false;
    for line in text.lines() {
        let mut rest = line;
        while !rest.is_empty() {
            if block {
                let Some((_, after)) = rest.split_once("*/") else {
                    rest = "";
                    continue;
                };
                block = false;
                rest = after;
            } else if let Some((before, after)) = rest.split_once("/*") {
                output.push_str(before.split("//").next().unwrap_or_default());
                block = true;
                rest = after;
            } else {
                output.push_str(rest.split("//").next().unwrap_or_default());
                rest = "";
            }
        }
        output.push('\n');
    }
    output
}
