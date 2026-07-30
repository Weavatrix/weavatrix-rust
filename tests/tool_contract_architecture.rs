#![cfg(all(
    feature = "clone",
    feature = "git",
    feature = "lang-rust",
    feature = "memory",
    feature = "search",
    feature = "semantic",
    feature = "vector"
))]

mod support;

use blazingly_json::{Value, json};
use support::GitFixture;
use weavatrix_rust::{Weavatrix, tools};

#[test]
fn absent_optional_configuration_and_routes_are_structured_results() {
    let fixture = GitFixture::new();
    fixture.write(
        "src/main.js",
        "export function main() { return 'configured by code only'; }\n",
    );
    fixture.commit("without architecture contract");
    let mut engine = Weavatrix::open(&fixture.root).unwrap();

    for (tool, args) in [
        (
            "prepare_change",
            json!({"files": ["src/main.js"], "intent": "inspect"}),
        ),
        ("verify_architecture", json!({})),
        (
            "explain_architecture_violation",
            json!({"fingerprint": "missing"}),
        ),
        (
            "propose_architecture_exception",
            json!({"fingerprint": "missing", "reason": "none"}),
        ),
    ] {
        let result = tools::call(&mut engine, tool, args)
            .unwrap_or_else(|error| panic!("{tool} must not expose an IO error: {error}"));
        assert_eq!(
            result["state"], "NOT_CONFIGURED",
            "{tool} must make the optional configuration state explicit"
        );
    }

    let missing = tools::call(
        &mut engine,
        "trace_endpoint",
        json!({"path": "/absent", "method": "GET"}),
    )
    .expect("an absent endpoint is a query result, not a tool failure");
    assert_eq!(missing["state"], "NOT_FOUND");
    assert_eq!(missing["endpoint"], Value::Null);
    assert_eq!(missing["nodes"], json!([]));
}

/// A contract written for the JavaScript engine names coupling kinds rather
/// than relation names. Matching nothing would report a passing verification,
/// so the vocabulary must either be evaluated or rejected out loud.
#[test]
fn coupling_kinds_are_evaluated_and_unknown_kinds_are_rejected() {
    let fixture = GitFixture::new();
    fixture.write("lib/util.ts", "export type Helper = { id: string };\n");
    fixture.write(
        "app/main.ts",
        "import type { Helper } from '../lib/util.ts';\nexport const use = (value: Helper) => value.id;\n",
    );
    fixture.write(
        "app/runtime.ts",
        "import { helper } from '../lib/impl.ts';\nexport const run = () => helper();\n",
    );
    fixture.write("lib/impl.ts", "export function helper(){ return 1; }\n");
    let contract = |kinds: &str| {
        format!(
            r#"{{"components":[{{"id":"app","paths":["app"]}},{{"id":"lib","paths":["lib"]}}],"dependencyRules":[{{"id":"no-app-lib","action":"forbid","from":["app"],"to":["lib"],"kinds":[{kinds}]}}],"ratchet":{{"baseline":{{"fingerprints":[]}}}}}}"#
        )
    };

    fixture.write(".weavatrix/architecture.json", &contract("\"runtime\""));
    let mut engine = Weavatrix::open(&fixture.root).unwrap();
    let runtime_only = tools::call(&mut engine, "verify_architecture", json!({})).unwrap();
    let flagged = runtime_only["new"]
        .as_array()
        .map(|items| {
            items
                .iter()
                .filter_map(|item| item["source"]["label"].as_str().map(str::to_owned))
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    assert!(
        flagged.iter().any(|label| label.contains("runtime.ts")),
        "a runtime import must violate a runtime rule, got {flagged:?}"
    );
    assert!(
        !flagged.iter().any(|label| label.contains("main.ts")),
        "an import type edge must not violate a runtime rule, got {flagged:?}"
    );

    fixture.write(".weavatrix/architecture.json", &contract("\"type-only\""));
    let mut engine = Weavatrix::open(&fixture.root).unwrap();
    let type_only = tools::call(&mut engine, "verify_architecture", json!({})).unwrap();
    let flagged = type_only["new"]
        .as_array()
        .map(|items| {
            items
                .iter()
                .filter_map(|item| item["source"]["label"].as_str().map(str::to_owned))
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    assert!(
        flagged.iter().any(|label| label.contains("main.ts")),
        "a type-only rule must catch the import type edge, got {flagged:?}"
    );

    fixture.write(
        ".weavatrix/architecture.json",
        &contract("\"compile-only\""),
    );
    let mut engine = Weavatrix::open(&fixture.root).unwrap();
    let error = tools::call(&mut engine, "verify_architecture", json!({}))
        .expect_err("an unevaluable kind must fail instead of passing");
    assert!(
        error.contains("compile-only"),
        "the rejection must name the unsupported kind, got {error}"
    );
}
