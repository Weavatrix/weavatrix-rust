//! Every advertised parameter changes the answer: the repo-lens audit found
//! several accepted and ignored, which is worse than rejecting them.

mod language_fixture;

use blazingly_json::json;
use language_fixture::Fixture;
use weavatrix_rust::{Weavatrix, tools};

fn small_app() -> Fixture {
    let fixture = Fixture::new();
    fixture.write(
        "src/hub.js",
        "import { helper } from './lib/util.js';\nexport function hub() {\n  let total = 0;\n  for (const step of [1, 2]) {\n    if (step > 0 && step < 9) {\n      total += helper(step);\n    }\n  }\n  return total;\n}\n",
    );
    fixture.write(
        "src/lib/util.js",
        "export function helper(value) { return value; }\nfunction spare() { return 0; }\n",
    );
    fixture
}

#[test]
fn get_neighbors_applies_an_array_relation_filter() {
    let fixture = small_app();
    let mut engine = Weavatrix::open(&fixture.root).unwrap();
    let report = tools::call(
        &mut engine,
        "get_neighbors",
        json!({"label": "file:src/hub.js", "relation_filter": ["imports"]}),
    )
    .unwrap();
    let neighbors = report["neighbors"].as_array().unwrap();
    assert!(
        !neighbors.is_empty(),
        "the import edge itself must be returned: {report}"
    );
    assert!(
        neighbors.iter().all(|item| item["relation"] == "imports"),
        "an array relation_filter must filter every edge kind: {report}"
    );
    let error = tools::call(
        &mut engine,
        "get_neighbors",
        json!({"label": "file:src/hub.js", "relation_filter": []}),
    )
    .unwrap_err();
    assert!(
        error.contains("relation_filter"),
        "an empty filter is a caller error, not an unfiltered answer: {error}"
    );
}

#[test]
fn hot_path_review_applies_min_score_and_thresholds() {
    let fixture = small_app();
    let mut engine = Weavatrix::open(&fixture.root).unwrap();
    let unfiltered = tools::call(&mut engine, "hot_path_review", json!({"top_n": 10})).unwrap();
    assert!(
        unfiltered["candidates"]
            .as_array()
            .is_some_and(|items| !items.is_empty()),
        "the fixture must produce candidates: {unfiltered}"
    );
    let ceiling = tools::call(
        &mut engine,
        "hot_path_review",
        json!({"top_n": 10, "min_score": 999_999}),
    )
    .unwrap();
    assert_eq!(
        ceiling["candidates"].as_array().unwrap().len(),
        0,
        "an unreachable min_score must return no candidates: {ceiling}"
    );
    let looping = tools::call(
        &mut engine,
        "hot_path_review",
        json!({"top_n": 10, "loop_depth_threshold": 1, "cyclomatic_threshold": 2}),
    )
    .unwrap();
    let names = looping["candidates"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|item| item["node"]["label"].as_str())
        .collect::<Vec<_>>();
    assert_eq!(
        names,
        vec!["hub"],
        "thresholds must keep only the looping, branching function: {looping}"
    );
}

#[test]
fn hot_path_review_measures_python_loop_nesting() {
    let fixture = Fixture::new();
    fixture.write(
        "src/job.py",
        "def crunch(rows):\n    total = 0\n    for row in rows:\n        for cell in row:\n            if cell and cell > 0:\n                total += cell\n    return total\n",
    );
    let mut engine = Weavatrix::open(&fixture.root).unwrap();
    let unfiltered = tools::call(&mut engine, "hot_path_review", json!({"top_n": 10})).unwrap();
    let report = tools::call(
        &mut engine,
        "hot_path_review",
        json!({"top_n": 10, "loop_depth_threshold": 2}),
    )
    .unwrap();
    let candidates = report["candidates"].as_array().unwrap();
    assert_eq!(
        candidates.len(),
        1,
        "only the nested-loop function reaches depth 2: {report}; unfiltered: {unfiltered}"
    );
    assert_eq!(candidates[0]["node"]["label"], "crunch");
    assert_eq!(candidates[0]["max_loop_depth"], 2_u64);
}

#[test]
fn dead_code_confidence_tiers_reach_the_advertised_scale() {
    let fixture = small_app();
    let mut engine = Weavatrix::open(&fixture.root).unwrap();
    let report = tools::call(
        &mut engine,
        "find_dead_code",
        json!({"top_n": 50, "min_confidence": 80, "kinds": ["function"]}),
    )
    .unwrap();
    let labels = report["candidates"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|item| item["node"]["label"].as_str())
        .collect::<Vec<_>>();
    assert_eq!(
        labels,
        vec!["spare"],
        "a private unreferenced function reaches confidence 85: {report}"
    );
}

#[test]
fn module_map_depth_groups_below_the_top_level() {
    let fixture = small_app();
    let mut engine = Weavatrix::open(&fixture.root).unwrap();
    let deep = tools::call(&mut engine, "module_map", json!({"depth": 2})).unwrap();
    let paths = deep["modules"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|module| module["path"].as_str().map(str::to_owned))
        .collect::<Vec<_>>();
    assert!(
        paths.contains(&"src/lib".to_owned()) && paths.contains(&"src".to_owned()),
        "depth 2 must split src and src/lib: {deep}"
    );
    let error = tools::call(&mut engine, "module_map", json!({"depth": 0})).unwrap_err();
    assert!(
        error.contains("depth"),
        "depth 0 is a caller error: {error}"
    );
}

#[test]
fn every_answer_names_its_repository_and_expected_repository_is_enforced() {
    let fixture = small_app();
    let mut engine = Weavatrix::open(&fixture.root).unwrap();
    let stats = tools::call(&mut engine, "graph_stats", json!({})).unwrap();
    let context = &stats["repository_context"];
    assert!(
        context["root"]
            .as_str()
            .is_some_and(|root| !root.is_empty()),
        "every answer names its repository root: {stats}"
    );
    assert!(
        context["scan_revision"]
            .as_str()
            .is_some_and(|revision| !revision.is_empty()),
        "every answer names the scanned revision: {stats}"
    );
    assert!(
        context["graph_age_seconds"].as_u64().is_some(),
        "every answer states its graph age: {stats}"
    );

    let folder = fixture
        .root
        .file_name()
        .unwrap()
        .to_string_lossy()
        .to_string();
    tools::call(
        &mut engine,
        "graph_stats",
        json!({"expected_repository": folder}),
    )
    .expect("the active repository folder name matches");
    let error = tools::call(
        &mut engine,
        "graph_stats",
        json!({"expected_repository": "somewhere-else"}),
    )
    .unwrap_err();
    assert!(
        error.contains("active repository") && error.contains("somewhere-else"),
        "a repository mismatch fails instead of answering: {error}"
    );
}

#[test]
fn a_commented_empty_catch_is_review_not_high_severity() {
    let fixture = Fixture::new();
    fixture.write(
        "src/main.js",
        "export function shutdown(client) {\n\
           try { client.close(); } catch { /* best-effort */ }\n\
           try { client.purge(); } catch {}\n\
         }\n",
    );
    fixture.write(
        "src/job.py",
        "def stop(worker):\n    try:\n        worker.kill()\n    except: pass  # already gone\n",
    );
    let mut engine = Weavatrix::open(&fixture.root).unwrap();
    let audit = tools::call(&mut engine, "run_audit", json!({"category": "runtime"})).unwrap();
    let findings = audit["runtime_report"]["findings"].as_array().unwrap();
    let mut severities = findings
        .iter()
        .filter(|finding| finding["rule"] == "runtime.empty_catch")
        .map(|finding| {
            (
                finding["file"].as_str().unwrap().to_owned(),
                finding["line"].as_u64().unwrap(),
                finding["severity"].as_str().unwrap().to_owned(),
            )
        })
        .collect::<Vec<_>>();
    severities.sort();
    assert_eq!(
        severities,
        vec![
            ("src/job.py".to_owned(), 4, "low".to_owned()),
            ("src/main.js".to_owned(), 2, "low".to_owned()),
            ("src/main.js".to_owned(), 3, "high".to_owned()),
        ],
        "a stated best-effort intent is review, silence stays high: {findings:?}"
    );
}
