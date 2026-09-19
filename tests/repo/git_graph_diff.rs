#![cfg(feature = "git")]

use crate::support::GitFixture;
use blazingly_json::json;
use weavatrix_rust::{Weavatrix, tools};

#[test]
fn compares_analyzed_worktree_with_immutable_git_objects() {
    let fixture = GitFixture::new();
    fixture.write("src/lib.rs", "pub fn existing() {}\n");
    fixture.commit("baseline");
    fixture.write(
        "src/lib.rs",
        "pub fn existing() {}\npub fn added() { existing(); }\n",
    );

    let mut engine = Weavatrix::open(&fixture.root).unwrap();
    let result = tools::call(
        &mut engine,
        "graph_diff",
        json!({"base_ref": "HEAD", "max_results": 20}),
    )
    .unwrap();

    assert_eq!(result["completeness"], "COMPLETE_FOR_SUPPORTED_LANGUAGES");
    assert_eq!(result["source_mutation"], "NONE");
    assert_eq!(result["git_process"], "NONE");
    assert!(result["counts"]["nodes_added"].as_u64().unwrap() >= 1);
    assert_eq!(
        result["counts"]["nodes_changed"], 1,
        "only the file whose byte length changed is structurally different; hash formats must not mark every file"
    );
    assert_eq!(
        result["nodes"]["changed"][0]["after"]["id"],
        "file:src/lib.rs"
    );
}

#[test]
fn change_impact_uses_the_active_worktree_by_default() {
    let fixture = GitFixture::new();
    fixture.write("src/lib.rs", "pub fn existing() {}\n");
    fixture.write("src/removed.rs", "pub fn removed() {}\n");
    fixture.commit("baseline");
    fixture.write(
        "src/lib.rs",
        "pub fn existing() { changed(); }\npub fn changed() {}\n",
    );
    std::fs::remove_file(fixture.root.join("src/removed.rs")).unwrap();
    fixture.write("src/added.rs", "pub fn added() {}\n");

    let mut engine = Weavatrix::open(&fixture.root).unwrap();
    let result = tools::call(&mut engine, "change_impact", json!({})).unwrap();
    let files = result["changed_files"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(blazingly_json::Value::as_str)
        .collect::<Vec<_>>();

    assert_eq!(result["git"]["head"], "WORKTREE");
    assert_eq!(
        files,
        ["src/added.rs", "src/lib.rs", "src/removed.rs"],
        "tracked, deleted and untracked supported sources are all included"
    );
}

#[test]
fn change_impact_returns_flat_dependent_nodes() {
    let fixture = GitFixture::new();
    fixture.write("services/init.js", "export function initialize() {}\n");
    fixture.write(
        "services/consumer.js",
        "import { initialize } from './init.js';\nexport function start() { initialize(); }\n",
    );
    fixture.write(
        "tests/consumer.test.js",
        "import { start } from '../services/consumer.js';\ntest('start', () => start());\n",
    );

    let mut engine = Weavatrix::open(&fixture.root).unwrap();
    let result = tools::call(
        &mut engine,
        "change_impact",
        json!({"files": ["services/init.js"], "depth": 2, "max_nodes": 20}),
    )
    .unwrap();
    let impacted = result["impacted_nodes"].as_array().unwrap();
    let ids = impacted
        .iter()
        .filter_map(|node| node["id"].as_str())
        .collect::<Vec<_>>();

    assert!(ids.contains(&"file:services/consumer.js"), "{impacted:?}");
    assert!(ids.contains(&"file:tests/consumer.test.js"), "{impacted:?}");
    assert!(
        impacted
            .iter()
            .all(|node| node.get("node").is_none() && node.get("id").is_some()),
        "change impact must expose the same flat node shape consumed by verified_change: {impacted:?}"
    );
}

#[test]
fn bounded_static_tools_reject_unavailable_lsp_precision() {
    let fixture = GitFixture::new();
    fixture.write("src/value.js", "export const value = 1;\n");
    let mut engine = Weavatrix::open(&fixture.root).unwrap();

    for (tool, arguments) in [
        (
            "get_dependents",
            json!({"label": "file:src/value.js", "precision": "lsp"}),
        ),
        (
            "change_impact",
            json!({"files": ["src/value.js"], "precision": "lsp"}),
        ),
    ] {
        let error = tools::call(&mut engine, tool, arguments).unwrap_err();
        assert!(
            error.contains("supports only 'graph' bounded static precision"),
            "{tool} must fail instead of silently downgrading lsp: {error}"
        );
    }
}

#[test]
fn change_impact_accepts_legacy_target_as_files() {
    let fixture = GitFixture::new();
    fixture.write("services/init.js", "export function initialize() {}\n");
    fixture.write(
        "services/consumer.js",
        "import { initialize } from './init.js';\nexport function start() { initialize(); }\n",
    );

    let mut engine = Weavatrix::open(&fixture.root).unwrap();
    let result = tools::call(
        &mut engine,
        "change_impact",
        json!({"target": "services/init.js", "depth": 2, "max_nodes": 20}),
    )
    .unwrap();
    assert_eq!(result["status"], "COMPLETE");
    assert_eq!(result["changed_files"], json!(["services/init.js"]));
    let ids = result["impacted_nodes"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|node| node["id"].as_str())
        .collect::<Vec<_>>();
    assert!(ids.contains(&"file:services/consumer.js"), "{ids:?}");
}

#[test]
fn change_impact_rejects_unknown_argument_instead_of_empty_complete() {
    let fixture = GitFixture::new();
    fixture.write("src/value.js", "export const value = 1;\n");
    let mut engine = Weavatrix::open(&fixture.root).unwrap();

    let error = tools::call(&mut engine, "change_impact", json!({"max_reslts": 3}))
        .expect_err("typo must not return COMPLETE");
    assert!(error.contains("max_reslts"), "{error}");
    assert!(error.contains("supported"), "{error}");

    let error = tools::call(
        &mut engine,
        "change_impact",
        json!({"output_format": "yaml"}),
    )
    .expect_err("invalid output_format");
    assert!(error.contains("output_format"), "{error}");
}

#[test]
fn verified_change_passes_an_unchanged_worktree_without_running_processes() {
    let fixture = GitFixture::new();
    fixture.write("src/lib.rs", "pub fn stable() {}\n");
    fixture.commit("baseline");

    let mut engine = Weavatrix::open(&fixture.root).unwrap();
    let result = tools::call(
        &mut engine,
        "verified_change",
        json!({"task": "verify stable tree", "phase": "verify", "base_ref": "HEAD"}),
    )
    .unwrap();

    assert_eq!(result["verdict"], "PASS");
    assert_eq!(
        result["changeImpact"]["changed_files"]
            .as_array()
            .unwrap()
            .len(),
        0
    );
    assert_eq!(result["source_mutation"], "NONE");
    assert_eq!(result["test_execution"]["present"], false);
    assert_eq!(
        result["test_execution"]["reason"],
        "no test command was requested"
    );
}
