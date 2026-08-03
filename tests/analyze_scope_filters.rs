mod language_fixture;

use language_fixture::Fixture;

#[test]
fn dead_code_and_hot_paths_honor_exact_file_and_subtree_scopes() {
    use blazingly_json::json;
    use weavatrix_rust::{Weavatrix, tools};

    let fixture = Fixture::new();
    fixture.write("src/main.js", "export function main(){ return 1; }\n");
    fixture.write(
        "src/left/orphan.js",
        "export function leftForgotten(){ return 2; }\n",
    );
    fixture.write(
        "src/right/orphan.js",
        "export function rightForgotten(){ return 3; }\n",
    );
    let mut engine = Weavatrix::open(&fixture.root).unwrap();

    let subtree = tools::call(
        &mut engine,
        "find_dead_code",
        json!({"path": "src\\left", "min_confidence": 50, "top_n": 50}),
    )
    .unwrap();
    let subtree_labels = subtree["candidates"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|item| item["node"]["label"].as_str())
        .collect::<Vec<_>>();
    assert!(subtree_labels.contains(&"leftForgotten"));
    assert!(!subtree_labels.contains(&"rightForgotten"));

    let exact = tools::call(
        &mut engine,
        "find_dead_code",
        json!({"path": "src/right/orphan.js", "min_confidence": 50, "top_n": 50}),
    )
    .unwrap();
    let exact_labels = exact["candidates"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|item| item["node"]["label"].as_str())
        .collect::<Vec<_>>();
    assert!(exact_labels.contains(&"rightForgotten"));
    assert!(!exact_labels.contains(&"leftForgotten"));

    let hot = tools::call(
        &mut engine,
        "hot_path_review",
        json!({"path": "src/left", "top_n": 50}),
    )
    .unwrap();
    assert!(
        hot["candidates"].as_array().is_some_and(|items| {
            !items.is_empty()
                && items.iter().all(|item| {
                    item["node"]["span"]["file"]
                        .as_str()
                        .is_some_and(|path| path == "src/left/orphan.js")
                })
        }),
        "hot paths escaped the requested subtree: {hot:?}"
    );
}

#[test]
fn tool_configuration_stays_in_inventory_but_not_in_default_dead_code() {
    use blazingly_json::json;
    use weavatrix_rust::{Analyzer, NodeKind, Weavatrix, tools};

    let fixture = Fixture::new();
    fixture.write("src/main.js", "export function main(){ return 1; }\n");
    fixture.write(
        "src/orphan.js",
        "export function genuinelyForgotten(){ return 2; }\n",
    );
    fixture.write(".vscode/settings.json", "{}\n");
    fixture.write("jsconfig.json", "{}\n");
    fixture.write("tsconfig.build.json", "{}\n");
    fixture.write("jest.config.cjs", "module.exports = {};\n");
    fixture.write("vite.config.ts", "export default {};\n");

    let snapshot = Analyzer::default().analyze(&fixture.root).unwrap();
    for path in [
        ".vscode/settings.json",
        "jsconfig.json",
        "tsconfig.build.json",
        "jest.config.cjs",
        "vite.config.ts",
    ] {
        assert!(
            snapshot
                .nodes
                .iter()
                .any(|node| node.kind == NodeKind::File && node.label == path),
            "{path} must remain in graph inventory"
        );
    }

    let mut engine = Weavatrix::open(&fixture.root).unwrap();
    let report = tools::call(
        &mut engine,
        "find_dead_code",
        json!({"min_confidence": 0, "top_n": 100}),
    )
    .unwrap();
    let candidate_ids = report["candidates"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|item| item["node"]["id"].as_str())
        .collect::<Vec<_>>();
    for configuration in [
        ".vscode/settings.json",
        "jsconfig.json",
        "tsconfig.build.json",
        "jest.config.cjs",
        "vite.config.ts",
    ] {
        assert!(
            candidate_ids.iter().all(|id| !id.contains(configuration)),
            "configuration is not dead code: {configuration}; got {candidate_ids:?}"
        );
    }
    assert!(
        candidate_ids
            .iter()
            .any(|id| id.contains("genuinelyForgotten")),
        "real source candidate disappeared: {report:?}"
    );
}

/// Tools whose schema offers `include_tests` / `include_classified` must
/// actually apply them: an advertised parameter that is ignored is a schema
/// that lies about the answer.
#[test]
fn production_first_filters_are_applied_by_the_tools_that_advertise_them() {
    use blazingly_json::json;
    use weavatrix_rust::{Weavatrix, tools};

    let fixture = Fixture::new();
    fixture.write(
        "src/service.js",
        "export function serve(){ return 1; }\nrouter.get('/live', serve);\n",
    );
    fixture.write(
        "src/__test__/service.test.js",
        "import { serve } from '../service.js';\nrouter.get('/only-in-tests', serve);\nexport function check(){ return serve(); }\n",
    );
    let mut engine = Weavatrix::open(&fixture.root).unwrap();

    let labels = |value: &blazingly_json::Value, pointer: &str| {
        value
            .pointer(pointer)
            .and_then(|items| items.as_array())
            .map(|items| {
                items
                    .iter()
                    .filter_map(|item| {
                        item.get("node")
                            .unwrap_or(item)
                            .get("label")
                            .and_then(|label| label.as_str())
                            .map(str::to_owned)
                    })
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default()
    };

    for (tool, pointer) in [("god_nodes", "/hubs"), ("hot_path_review", "/candidates")] {
        let production = tools::call(&mut engine, tool, json!({"top_n": 50})).unwrap();
        let with_tests = tools::call(
            &mut engine,
            tool,
            json!({"top_n": 50, "include_tests": true}),
        )
        .unwrap();
        let production = labels(&production, pointer);
        let with_tests = labels(&with_tests, pointer);
        assert!(
            !production.iter().any(|label| label.contains("test")),
            "{tool} must not rank test evidence by default, got {production:?}"
        );
        assert!(
            with_tests.len() >= production.len(),
            "{tool} include_tests must widen the answer, got {with_tests:?}"
        );
    }

    let production = tools::call(&mut engine, "list_endpoints", json!({})).unwrap();
    let production = labels(&production, "/endpoints");
    assert!(
        production.iter().any(|label| label.contains("/live")),
        "a production route stays listed, got {production:?}"
    );
    assert!(
        !production
            .iter()
            .any(|label| label.contains("/only-in-tests")),
        "a route declared only in a test is not a production endpoint, got {production:?}"
    );
    let with_tests = tools::call(
        &mut engine,
        "list_endpoints",
        json!({"include_tests": true}),
    )
    .unwrap();
    assert!(
        labels(&with_tests, "/endpoints")
            .iter()
            .any(|label| label.contains("/only-in-tests")),
        "include_tests reveals the test-only route"
    );
}
