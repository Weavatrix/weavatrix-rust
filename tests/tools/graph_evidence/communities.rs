use crate::language_fixture::Fixture;
use blazingly_json::json;
use weavatrix_rust::{Weavatrix, tools};

fn write_bridge(fixture: &Fixture) {
    fixture.write(
        "left/a.js",
        "import { b } from './b.js';\nimport { x } from '../right/x.js';\nexport function a() { return b() + x(); }\n",
    );
    fixture.write(
        "left/b.js",
        "import { c } from './c.js';\nexport function b() { return c(); }\n",
    );
    fixture.write("left/c.js", "export function c() { return 1; }\n");
    fixture.write(
        "right/x.js",
        "import { y } from './y.js';\nexport function x() { return y(); }\n",
    );
    fixture.write(
        "right/y.js",
        "import { z } from './z.js';\nexport function y() { return z(); }\n",
    );
    fixture.write("right/z.js", "export function z() { return 2; }\n");
}

fn keys(report: &blazingly_json::Value) -> Vec<String> {
    report["communities"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|item| item["community_key"].as_str().map(str::to_owned))
        .collect()
}

#[test]
fn list_communities_keeps_two_modules_across_a_single_bridge() {
    let fixture = Fixture::new();
    write_bridge(&fixture);
    fixture.write(
        ".weavatrix/architecture.json",
        r#"{"components":[{"id":"left","paths":["left"]},{"id":"right","paths":["right"]}]}"#,
    );
    let mut engine = Weavatrix::open(&fixture.root).unwrap();
    let report = tools::call(&mut engine, "list_communities", json!({})).unwrap();
    assert_eq!(report["view"], "subsystems");
    let listed = keys(&report);
    assert!(
        listed.iter().any(|key| key == "left") && listed.iter().any(|key| key == "right"),
        "{report}"
    );
    assert!(
        report["communities"]
            .as_array()
            .unwrap()
            .iter()
            .all(|item| item["kind"] == "subsystem"),
        "{report}"
    );
}

#[test]
fn list_communities_does_not_let_a_logger_merge_the_repository() {
    let fixture = Fixture::new();
    fixture.write(
        "log.js",
        "export function log(message) { return message; }\n",
    );
    fixture.write(
        "left/a.js",
        "import { log } from '../log.js';\nimport { b } from './b.js';\nexport function a() { return log(b()); }\n",
    );
    fixture.write(
        "left/b.js",
        "import { log } from '../log.js';\nexport function b() { return log(1); }\n",
    );
    fixture.write(
        "right/x.js",
        "import { log } from '../log.js';\nimport { y } from './y.js';\nexport function x() { return log(y()); }\n",
    );
    fixture.write(
        "right/y.js",
        "import { log } from '../log.js';\nexport function y() { return log(2); }\n",
    );
    let mut engine = Weavatrix::open(&fixture.root).unwrap();
    let report = tools::call(&mut engine, "list_communities", json!({})).unwrap();
    let listed = keys(&report);
    assert!(listed.iter().any(|key| key == "left"), "{report}");
    assert!(listed.iter().any(|key| key == "right"), "{report}");
    assert!(
        report["communities"]
            .as_array()
            .unwrap()
            .iter()
            .any(|item| item["kind"] == "utility"),
        "{report}"
    );
}

#[test]
fn list_communities_keeps_keys_when_an_unrelated_group_is_added() {
    let first = Fixture::new();
    write_bridge(&first);
    let mut engine = Weavatrix::open(&first.root).unwrap();
    let before = tools::call(&mut engine, "list_communities", json!({})).unwrap();
    let previous = keys(&before)
        .into_iter()
        .collect::<std::collections::BTreeSet<_>>();
    let second = Fixture::new();
    write_bridge(&second);
    second.write(
        "extra/p.js",
        "import { q } from './q.js';\nexport function p() { return q(); }\n",
    );
    second.write("extra/q.js", "export function q() { return 3; }\n");
    let mut engine = Weavatrix::open(&second.root).unwrap();
    let after = tools::call(&mut engine, "list_communities", json!({})).unwrap();
    let later = keys(&after)
        .into_iter()
        .collect::<std::collections::BTreeSet<_>>();
    assert!(previous.is_subset(&later), "before={before} after={after}");
}

#[test]
fn list_communities_is_stable_across_write_order() {
    let left_first = Fixture::new();
    write_bridge(&left_first);
    let right_first = Fixture::new();
    right_first.write("right/z.js", "export function z() { return 2; }\n");
    write_bridge(&right_first);
    let mut first = Weavatrix::open(&left_first.root).unwrap();
    let mut second = Weavatrix::open(&right_first.root).unwrap();
    let a = tools::call(&mut first, "list_communities", json!({})).unwrap();
    let b = tools::call(&mut second, "list_communities", json!({})).unwrap();
    assert_eq!(a["communities"], b["communities"]);
}
