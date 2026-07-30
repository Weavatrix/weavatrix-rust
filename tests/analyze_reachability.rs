mod language_fixture;

use language_fixture::Fixture;

/// Dead-code review must answer "unreachable from any way in", not "nothing
/// imports it": the latter flags a package's own executables and its CI.
#[test]
fn dead_code_starts_from_declared_entry_points() {
    use blazingly_json::json;
    use weavatrix_rust::{Weavatrix, tools};

    let fixture = Fixture::new();
    fixture.write(
        "package.json",
        r#"{"name":"app","main":"src/index.js","bin":{"app":"bin/cli.js"}}"#,
    );
    fixture.write(
        "src/index.js",
        "import { serve } from './server.js';\nexport const boot = () => serve();\n",
    );
    fixture.write("src/server.js", "export function serve(){ return 1; }\n");
    fixture.write("bin/cli.js", "import '../src/index.js';\n");
    fixture.write(
        ".github/workflows/ci.yml",
        "name: CI\njobs:\n  build:\n    runs-on: ubuntu-latest\n",
    );
    fixture.write(
        "src/orphan.js",
        "export function forgotten(){ return 2; }\n",
    );

    let mut engine = Weavatrix::open(&fixture.root).unwrap();
    let report = tools::call(&mut engine, "find_dead_code", json!({"top_n": 50})).unwrap();
    let candidates = report["candidates"]
        .as_array()
        .map(|items| {
            items
                .iter()
                .filter_map(|item| item["node"]["id"].as_str().map(str::to_owned))
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();

    for reachable in [
        "file:package.json",
        "file:bin/cli.js",
        "file:src/index.js",
        "file:src/server.js",
        "file:.github/workflows/ci.yml",
    ] {
        assert!(
            !candidates.iter().any(|id| id == reachable),
            "{reachable} must not be reported as dead, got {candidates:?}"
        );
    }
    assert!(
        candidates.iter().any(|id| id == "file:src/orphan.js"),
        "a genuinely unreachable module is still reported, got {candidates:?}"
    );
    assert!(
        report["entry_points"]
            .as_array()
            .is_some_and(|entries| entries.len() >= 2),
        "the entry points used are reported so the claim is auditable"
    );
}

#[test]
fn dead_code_applies_kind_and_confidence_filters() {
    use blazingly_json::json;
    use weavatrix_rust::{Weavatrix, tools};

    let fixture = Fixture::new();
    fixture.write("src/main.js", "export function main(){ return 1; }\n");
    fixture.write(
        "src/orphan.js",
        "export function forgotten(){ return 2; }\n",
    );
    let mut engine = Weavatrix::open(&fixture.root).unwrap();

    let confident = tools::call(
        &mut engine,
        "find_dead_code",
        json!({"top_n": 50, "min_confidence": 50}),
    )
    .unwrap();
    let candidates = confident["candidates"].as_array().unwrap();
    assert!(
        candidates
            .iter()
            .all(|item| item["confidence_score"].as_u64() == Some(50)),
        "the threshold must remove low-confidence file candidates: {candidates:?}"
    );
    assert!(
        candidates
            .iter()
            .any(|item| item["node"]["label"] == "forgotten"),
        "the medium-confidence symbol candidate remains visible"
    );

    let files = tools::call(
        &mut engine,
        "find_dead_code",
        json!({"top_n": 50, "kinds": ["file"]}),
    )
    .unwrap();
    assert!(
        files["candidates"]
            .as_array()
            .is_some_and(|items| items.iter().all(|item| item["node"]["kind"] == "file")),
        "kinds must constrain the returned node kinds"
    );
}

#[test]
fn dead_code_excludes_inline_cfg_test_modules_in_product_files() {
    use blazingly_json::json;
    use weavatrix_rust::{Weavatrix, tools};

    let fixture = Fixture::new();
    fixture.write(
        "cargo-blazingly/src/main.rs",
        "fn main() {}\nfn genuinely_unused() {}\n\n#[cfg(test)]\nmod tests {\n    #[test]\n    fn embedded_test() {}\n\n    fn helper_for_test() {}\n}\n\n#[cfg(not(not(test)))]\nfn double_negated_test() {}\n",
    );

    let mut engine = Weavatrix::open(&fixture.root).unwrap();
    let candidates = |report: &blazingly_json::Value| {
        report["candidates"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|item| item["node"]["label"].as_str().map(str::to_owned))
            .collect::<Vec<_>>()
    };
    let production = tools::call(&mut engine, "find_dead_code", json!({"top_n": 50})).unwrap();
    let with_tests = tools::call(
        &mut engine,
        "find_dead_code",
        json!({"top_n": 50, "include_tests": true}),
    )
    .unwrap();

    let production = candidates(&production);
    let with_tests = candidates(&with_tests);
    assert!(
        production.iter().any(|label| label == "genuinely_unused"),
        "real production dead code must remain visible, got {production:?}"
    );
    for test_symbol in ["embedded_test", "helper_for_test", "double_negated_test"] {
        assert!(
            !production.iter().any(|label| label == test_symbol),
            "{test_symbol} inherits #[cfg(test)] and must not be a production candidate"
        );
        assert!(
            with_tests.iter().any(|label| label == test_symbol),
            "include_tests must reveal {test_symbol}, got {with_tests:?}"
        );
    }
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
