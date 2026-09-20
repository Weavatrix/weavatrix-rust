#![cfg(feature = "git")]

use crate::support::GitFixture;
use blazingly_json::json;
use weavatrix_rust::{Weavatrix, tools};

#[test]
fn graph_diff_names_a_public_symbol_whose_body_changed() {
    let fixture = GitFixture::new();
    fixture.write(
        "src/lib.rs",
        "pub fn permission(a: i32, b: i32) -> i32 { a + b }\n\
         fn private_helper() -> bool { false }\n",
    );
    fixture.commit("baseline");
    fixture.write(
        "src/lib.rs",
        "pub fn permission(a: i32, b: i32) -> i32 { a - b }\n\
         fn private_helper() -> bool { false }\n",
    );

    let mut engine = Weavatrix::open(&fixture.root).unwrap();
    let diff = tools::call(
        &mut engine,
        "graph_diff",
        json!({"base_ref": "HEAD", "detail": "edges", "max_results": 20}),
    )
    .unwrap();
    let changed = diff["nodes"]["changed"].as_array().unwrap();
    let public = changed
        .iter()
        .find(|item| item["after"]["label"] == "permission")
        .expect("the changed declaration is named, not only its file");

    assert_eq!(public["after"]["attributes"]["exported"], true);
    assert!(
        changed
            .iter()
            .all(|item| item["after"]["label"] != "private_helper")
    );
}

#[test]
fn graph_diff_names_an_exported_go_symbol_whose_body_changed() {
    let fixture = GitFixture::new();
    fixture.write(
        "service/auth.go",
        "package service\n\nfunc CanDelete(viewer bool) bool { return !viewer }\n",
    );
    fixture.commit("baseline");
    fixture.write(
        "service/auth.go",
        "package service\n\nfunc CanDelete(viewer bool) bool { return viewer }\n",
    );

    let mut engine = Weavatrix::open(&fixture.root).unwrap();
    let diff = tools::call(
        &mut engine,
        "graph_diff",
        json!({"base_ref": "HEAD", "detail": "edges", "max_results": 20}),
    )
    .unwrap();
    let changed = diff["nodes"]["changed"].as_array().unwrap();
    let public = changed
        .iter()
        .find(|item| item["after"]["label"] == "CanDelete")
        .expect("the changed Go declaration is named, not only its file");

    assert_eq!(public["after"]["attributes"]["exported"], true);
}

#[test]
fn graph_diff_rolls_edge_churn_up_to_file_pairs_by_default() {
    let fixture = GitFixture::new();
    fixture.write("src/lib.rs", "mod caller;\nmod target;\n");
    fixture.write(
        "src/caller.rs",
        "use crate::target::target;\npub fn caller() {}\n",
    );
    fixture.write("src/target.rs", "pub fn target() {}\n");
    fixture.commit("baseline");
    fixture.write(
        "src/caller.rs",
        "use crate::target::target;\npub fn caller() { target(); }\n",
    );

    let mut engine = Weavatrix::open(&fixture.root).unwrap();
    let compact = tools::call(&mut engine, "graph_diff", json!({"base_ref": "HEAD"})).unwrap();
    let pairs = compact["edges"]["by_file"].as_array().unwrap();

    assert_eq!(compact["edges"]["detail"], "file_pairs");
    assert!(
        pairs.iter().any(|pair| {
            pair["from"] == "src/caller.rs"
                && pair["to"] == "src/target.rs"
                && pair["relation"] == "calls"
                && pair["added"] == 1
                && pair["removed"] == 0
        }),
        "the compact diff must retain the actionable cross-file call change: {pairs:?}"
    );

    let detailed = tools::call(
        &mut engine,
        "graph_diff",
        json!({"base_ref": "HEAD", "detail": "edges"}),
    )
    .unwrap();
    assert_eq!(detailed["edges"]["detail"], "edges");
    assert!(detailed["edges"]["added"].as_array().is_some());
    assert!(detailed["edges"]["by_file"].is_null());
}

fn many_functions(changed_body: &str) -> String {
    let mut source = String::new();
    for index in 0..40 {
        source.push_str(&format!(
            "export function fn{index}() {{ return {index}; }}\n"
        ));
    }
    source.push_str("export function target() { ");
    source.push_str(changed_body);
    source.push_str(" }\n");
    source
}

#[test]
fn change_impact_names_one_changed_function_not_its_neighbors() {
    let fixture = GitFixture::new();
    fixture.write("src/lib.js", &many_functions("return 1;"));
    fixture.commit("baseline");
    fixture.write("src/lib.js", &many_functions("return 2;"));
    let mut engine = Weavatrix::open(&fixture.root).unwrap();
    let result = tools::call(&mut engine, "change_impact", json!({"base_ref": "HEAD"})).unwrap();
    let labels = result["changed_symbols"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|item| item["change"] == "body" || item["change"] == "signature")
        .filter_map(|item| item["label"].as_str())
        .collect::<Vec<_>>();
    assert_eq!(
        labels,
        ["target"],
        "only the edited function is a body change: {result}"
    );
    assert_eq!(result["granularity"], "symbol");
}

#[test]
fn change_impact_reads_deleted_callers_from_the_baseline() {
    let fixture = GitFixture::new();
    fixture.write(
        "src/init.js",
        "export function initialize() { return 1; }\n",
    );
    fixture.write(
        "src/consumer.js",
        "import { initialize } from './init.js';\nexport function start() { return initialize(); }\n",
    );
    fixture.commit("baseline");
    fixture.write("src/init.js", "export function leftover() { return 0; }\n");
    let mut engine = Weavatrix::open(&fixture.root).unwrap();
    let result = tools::call(&mut engine, "change_impact", json!({"base_ref": "HEAD"})).unwrap();
    let removed = result["changed_symbols"]
        .as_array()
        .unwrap()
        .iter()
        .any(|item| item["label"] == "initialize" && item["change"] == "removed");
    assert!(removed, "the deleted export must be named: {result}");
    let ids = result["impacted_nodes"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|node| node["id"].as_str())
        .collect::<Vec<_>>();
    assert!(
        ids.iter()
            .any(|id| id.contains("consumer") || id.contains("start")),
        "baseline callers must survive a deletion: {ids:?} {result}"
    );
}

#[test]
fn change_impact_does_not_treat_a_comment_as_behavior() {
    let fixture = GitFixture::new();
    fixture.write("src/keep.js", "export function keep() { return 1; }\n");
    fixture.commit("baseline");
    fixture.write(
        "src/keep.js",
        "// note only\nexport function keep() { return 1; }\n",
    );
    let mut engine = Weavatrix::open(&fixture.root).unwrap();
    let result = tools::call(&mut engine, "change_impact", json!({"base_ref": "HEAD"})).unwrap();
    let behavioral = result["changed_symbols"]
        .as_array()
        .unwrap()
        .iter()
        .any(|item| {
            matches!(
                item["change"].as_str(),
                Some("body" | "signature" | "export")
            )
        });
    assert!(
        !behavioral,
        "a file comment is not a proven body change: {result}"
    );
}

#[test]
fn change_impact_does_not_mix_an_explicit_head_with_a_dirty_worktree() {
    let fixture = GitFixture::new();
    fixture.write(
        "src/pair.js",
        "export function foo() { return 1; }\nexport function bar() { return 1; }\n",
    );
    fixture.commit("baseline");
    fixture.write(
        "src/pair.js",
        "export function foo() { return 2; }\nexport function bar() { return 1; }\n",
    );
    fixture.commit("head");
    fixture.write(
        "src/pair.js",
        "export function foo() { return 2; }\nexport function bar() { return 9; }\n",
    );
    let mut engine = Weavatrix::open(&fixture.root).unwrap();
    let result = tools::call(
        &mut engine,
        "change_impact",
        json!({"base_ref": "HEAD~1", "head_ref": "HEAD"}),
    )
    .unwrap();
    let labels = result["changed_symbols"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|item| item["change"] == "body")
        .filter_map(|item| item["label"].as_str())
        .collect::<Vec<_>>();
    assert_eq!(
        labels,
        ["foo"],
        "the dirty bar() must not leak into HEAD: {result}"
    );
    assert_eq!(result["candidate"]["revision"], "HEAD");
    assert_ne!(result["git"]["head"], "WORKTREE");
}
