use crate::language_fixture::Fixture;
use blazingly_json::json;
use weavatrix_rust::{Weavatrix, tools};

#[test]
fn shortest_path_returns_witnesses_and_honors_max_hops() {
    let fixture = Fixture::new();
    fixture.write(
        "src/a.js",
        "import { b } from './b.js';\nexport function a() { return b(); }\n",
    );
    fixture.write(
        "src/b.js",
        "import { c } from './c.js';\nexport function b() { return c(); }\n",
    );
    fixture.write("src/c.js", "export function c() { return 1; }\n");
    let mut engine = Weavatrix::open(&fixture.root).unwrap();
    let found = tools::call(
        &mut engine,
        "shortest_path",
        json!({"source": "a", "target": "c", "max_hops": 4}),
    )
    .unwrap();
    assert_eq!(found["found"], true);
    assert!(
        found["witnesses"]
            .as_array()
            .is_some_and(|hops| !hops.is_empty()),
        "a found path must name each hop: {found}"
    );
    let bounded = tools::call(
        &mut engine,
        "shortest_path",
        json!({"source": "a", "target": "c", "max_hops": 1}),
    )
    .unwrap();
    assert_eq!(bounded["found"], false);
    assert_eq!(bounded["bounded_out"], true);
    assert_eq!(bounded["stop_reason"], "MAX_HOPS");
    assert_eq!(found["path_kind"], "calls");
    assert_eq!(found["is_execution_chain"], true);
    let budget = tools::call(
        &mut engine,
        "shortest_path",
        json!({"source": "a", "target": "c", "max_edges": 1}),
    )
    .unwrap();
    assert_eq!(budget["found"], false);
    assert_eq!(budget["stop_reason"], "MAX_EDGES");
}

#[test]
fn shortest_path_does_not_follow_a_reverse_call_when_directed() {
    let fixture = Fixture::new();
    fixture.write(
        "src/a.js",
        "import { b } from './b.js';\nexport function a() { return b(); }\n",
    );
    fixture.write("src/b.js", "export function b() { return 1; }\n");
    let mut engine = Weavatrix::open(&fixture.root).unwrap();
    let reverse = tools::call(
        &mut engine,
        "shortest_path",
        json!({"source": "b", "target": "a", "directed": true, "path_kind": "calls"}),
    )
    .unwrap();
    assert_eq!(reverse["found"], false);
    let forward = tools::call(
        &mut engine,
        "shortest_path",
        json!({"source": "a", "target": "b", "directed": true, "path_kind": "calls"}),
    )
    .unwrap();
    assert_eq!(forward["found"], true);
    assert_eq!(forward["path_kind"], "calls");
    assert_eq!(forward["is_execution_chain"], true);
}

#[test]
fn shortest_path_keeps_parallel_relations_as_separate_witnesses() {
    let fixture = Fixture::new();
    fixture.write(
        "src/a.js",
        "import { b } from './b.js';\nexport { b };\nexport function a() { return b(); }\n",
    );
    fixture.write("src/b.js", "export function b() { return 1; }\n");
    let mut engine = Weavatrix::open(&fixture.root).unwrap();
    let report = tools::call(
        &mut engine,
        "shortest_path",
        json!({"source": "src/a.js", "target": "src/b.js", "flow_direction": "both"}),
    )
    .unwrap();
    let relations = report["witnesses"][0]["relations"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|item| item["relation"].as_str())
        .collect::<std::collections::BTreeSet<_>>();
    assert!(
        relations.len() >= 2,
        "file-level import and re-export must stay distinct: {report}"
    );
}
