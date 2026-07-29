#[cfg(feature = "git")]
mod support;

use blazingly_json::Value;
#[cfg(feature = "git")]
use blazingly_json::json;
use std::fs;
use std::path::Path;
#[cfg(feature = "git")]
use support::GitFixture;
#[cfg(feature = "git")]
use weavatrix_rust::Weavatrix;
use weavatrix_rust::tools;

#[test]
fn production_tool_sources_and_catalog_schemas_have_no_unknown_state_literal() {
    let tools_root = Path::new(env!("CARGO_MANIFEST_DIR")).join("src/tools");
    assert_sources_have_no_unknown_state_literal(&tools_root);

    for definition in tools::catalog() {
        assert_ne!(definition.name, "UNKNOWN");
        assert_ne!(definition.description, "UNKNOWN");
        assert_no_unknown_value(
            &definition.input_schema,
            &format!("{} input schema", definition.name),
        );
    }
}

#[cfg(feature = "git")]
#[test]
fn workflow_and_trace_results_use_explicit_evidence_states() {
    let backend = GitFixture::new();
    backend.write(
        "src/server.js",
        "export function items() { return 'v1'; }\nrouter.get('/api/items', items);\n",
    );
    backend.commit("baseline");
    backend.write(
        "src/server.js",
        "export function items() { return 'v2'; }\nrouter.get('/api/items', items);\n",
    );

    let client = GitFixture::new();
    client.write("src/client.js", "fetch('/api/items');\n");
    client.commit("client");

    let mut engine = Weavatrix::open(&backend.root).unwrap();
    let endpoint = tools::call(
        &mut engine,
        "trace_endpoint",
        json!({"path": "/api/items", "method": "GET"}),
    )
    .unwrap();
    assert_eq!(endpoint["dynamic_dispatch"]["evaluated"], true);
    assert_no_incomplete_value(&endpoint, "trace_endpoint result");

    let contract = tools::call(
        &mut engine,
        "trace_api_contract",
        json!({
            "backend": backend.root,
            "clients": [client.root],
            "transport": "http"
        }),
    )
    .unwrap();
    assert_eq!(contract["dynamic_contracts"]["evaluated"], true);
    assert_no_incomplete_value(&contract, "trace_api_contract result");

    let plan = tools::call(
        &mut engine,
        "verified_change",
        json!({"task": "update the endpoint", "phase": "plan", "base_ref": "HEAD"}),
    )
    .unwrap();
    assert_eq!(plan["status"], "COMPLETE");
    assert_eq!(plan["verdict"], "PLANNED");
    assert_no_incomplete_value(&plan, "verified_change plan result");

    let verification = tools::call(
        &mut engine,
        "verified_change",
        json!({"task": "update the endpoint", "phase": "verify", "base_ref": "HEAD"}),
    )
    .unwrap();
    assert_no_incomplete_value(&verification, "verified_change verify result");

    let cursor_error = tools::call(
        &mut engine,
        "trace_api_contract",
        json!({
            "backend": backend.root,
            "clients": [client.root],
            "transport": "http",
            "cursor": "invalid"
        }),
    )
    .unwrap_err();
    assert!(cursor_error.contains("cursor format is invalid"));

    let execution_error = tools::call(
        &mut engine,
        "verified_change",
        json!({
            "task": "run tests",
            "phase": "plan",
            "base_ref": "HEAD",
            "tests": ["cargo test"],
            "run_tests": true
        }),
    )
    .unwrap_err();
    assert!(execution_error.contains("run_tests=true is invalid"));
}

#[cfg(feature = "git")]
#[test]
fn absent_external_evidence_is_structured_and_malformed_evidence_is_an_error() {
    let fixture = GitFixture::new();
    fixture.write("src/lib.rs", "pub fn stable() {}\n");
    fixture.commit("baseline");
    let mut engine = Weavatrix::open(&fixture.root).unwrap();

    let audit = tools::call(&mut engine, "run_audit", json!({})).unwrap();
    assert_eq!(audit["execution"]["status"], "COMPLETE");
    assert_eq!(
        audit["coverage_report"]["measured_coverage"]["present"],
        false
    );
    assert!(
        audit["coverage_report"]["measured_coverage"]["reason"]
            .as_str()
            .is_some_and(|reason| reason.contains("no supported measured coverage report"))
    );
    assert_eq!(
        audit["runtime_report"]["runtime_evidence"]["present"],
        false
    );
    assert_eq!(audit["debt"]["status"], "COMPLETE");
    assert_eq!(audit["debt"]["comparison"]["present"], false);
    assert_no_incomplete_value(&audit, "run_audit result");

    let history = tools::call(
        &mut engine,
        "git_history",
        json!({"max_commits": 10, "months": 1200}),
    )
    .unwrap();
    assert_eq!(history["status"], "COMPLETE");
    assert_eq!(history["git_evidence"]["present"], true);
    assert_eq!(history["analytics"]["numstat_lines"]["present"], false);
    assert!(
        history["analytics"]["numstat_lines"]["reason"]
            .as_str()
            .is_some_and(|reason| reason.contains("line-addition"))
    );
    assert_no_incomplete_value(&history, "git_history result");

    let argument_error =
        tools::call(&mut engine, "run_audit", json!({"max_findings": "many"})).unwrap_err();
    assert!(argument_error.contains("max_findings must be a non-negative integer"));
    let history_argument_error =
        tools::call(&mut engine, "git_history", json!({"months": "six"})).unwrap_err();
    assert!(history_argument_error.contains("months must be a non-negative integer"));

    fixture.write("lcov.info", "this is not lcov\n");
    let coverage_error = tools::call(&mut engine, "coverage_map", json!({})).unwrap_err();
    assert!(coverage_error.contains("LCOV contains no complete source records"));
}

fn assert_sources_have_no_unknown_state_literal(directory: &Path) {
    for entry in fs::read_dir(directory).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            assert_sources_have_no_unknown_state_literal(&path);
        } else if path.extension().is_some_and(|extension| extension == "rs") {
            let source = fs::read_to_string(&path).unwrap();
            for forbidden in [
                "\"UNKNOWN\"",
                "\"UNSUPPORTED\"",
                "\"NOT_SUPPORTED\"",
                "\"PARTIAL\"",
                "\"NOT_AVAILABLE\"",
                "\"unknown\"",
                "\"unknowns\"",
                "\"resolvedUnknowns\"",
            ] {
                assert!(
                    !source.contains(forbidden),
                    "{} contains the forbidden fallback literal {forbidden}",
                    path.display()
                );
            }
        }
    }
}

fn assert_no_unknown_value(value: &Value, location: &str) {
    match value {
        Value::String(text) => {
            assert_ne!(text, "UNKNOWN", "{location} emitted UNKNOWN");
        }
        Value::Array(items) => {
            for item in items {
                assert_no_unknown_value(item, location);
            }
        }
        Value::Object(entries) => {
            for (key, nested) in entries {
                assert_no_unknown_value(nested, &format!("{location}.{key}"));
            }
        }
        _ => {}
    }
}

#[cfg(feature = "git")]
fn assert_no_incomplete_value(value: &Value, location: &str) {
    match value {
        Value::String(text) => {
            assert!(
                !matches!(
                    text.as_str(),
                    "UNKNOWN" | "UNSUPPORTED" | "NOT_SUPPORTED" | "PARTIAL" | "NOT_AVAILABLE"
                ),
                "{location} emitted incomplete capability state {text}"
            );
        }
        Value::Array(items) => {
            for item in items {
                assert_no_incomplete_value(item, location);
            }
        }
        Value::Object(entries) => {
            for (key, nested) in entries {
                assert_no_incomplete_value(nested, &format!("{location}.{key}"));
            }
        }
        _ => {}
    }
}
