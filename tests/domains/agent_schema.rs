use crate::agent_support::{Fixture, dump};
use blazingly_json::json;
use weavatrix_rust::{Weavatrix, tools};

#[test]
fn type_change_is_not_proven_compatible() {
    let fixture = Fixture::empty();
    write_schema(
        &fixture,
        "catalogs/before.json",
        r#"{"type":"object","properties":{"id":{"type":"string"}},"required":["id"]}"#,
        true,
    );
    write_schema(
        &fixture,
        "catalogs/after.json",
        r#"{"type":"object","properties":{"id":{"type":"integer"}},"required":["id"]}"#,
        true,
    );
    let mut engine = Weavatrix::open(&fixture.root).unwrap();
    let report = tools::call(
        &mut engine,
        "agent_change_impact",
        json!({"before": "catalogs/before.json", "after": "catalogs/after.json"}),
    )
    .unwrap();
    let change = report["changes"].as_array().unwrap()[0].clone();
    assert_eq!(
        change["compatibility"],
        "proven-incompatible",
        "{}",
        dump(&report)
    );
    assert_eq!(change["direction"], "before-request-accepted-by-after");
}

#[test]
fn unsupported_union_is_undetermined() {
    let fixture = Fixture::empty();
    write_schema(
        &fixture,
        "catalogs/before.json",
        r#"{"type":"object","properties":{"id":{"type":"string"}},"required":["id"]}"#,
        true,
    );
    write_schema(
        &fixture,
        "catalogs/after.json",
        r#"{"type":"object","properties":{"id":{"anyOf":[{"type":"string"},{"type":"integer"}]}},"required":["id"]}"#,
        true,
    );
    let mut engine = Weavatrix::open(&fixture.root).unwrap();
    let report = tools::call(
        &mut engine,
        "agent_change_impact",
        json!({"before": "catalogs/before.json", "after": "catalogs/after.json"}),
    )
    .unwrap();
    assert_eq!(
        report["changes"][0]["compatibility"],
        "undetermined",
        "{}",
        dump(&report)
    );
}

#[test]
fn missing_tool_in_partial_after_is_unconfirmed() {
    let fixture = Fixture::empty();
    write_named(
        &fixture,
        "catalogs/before.json",
        "lookup",
        r#"{"type":"object","properties":{"id":{"type":"string"}},"required":["id"]}"#,
        true,
    );
    fixture.write(
        "catalogs/after.json",
        r#"{
          "$schema": "https://weavatrix.dev/schemas/agent-catalog/1.json",
          "scope": "after",
          "complete": false,
          "nextCursor": "page-2",
          "tools": []
        }"#,
    );
    let mut engine = Weavatrix::open(&fixture.root).unwrap();
    let report = tools::call(
        &mut engine,
        "agent_change_impact",
        json!({"before": "catalogs/before.json", "after": "catalogs/after.json"}),
    )
    .unwrap();
    let change = &report["changes"][0];
    assert_eq!(change["change"], "unconfirmed", "{}", dump(&report));
    assert_eq!(change["compatibility"], "undetermined", "{}", dump(&report));
}

#[test]
fn empty_catalog_path_is_not_complete_removal() {
    let fixture = Fixture::empty();
    write_named(
        &fixture,
        "catalogs/before.json",
        "lookup",
        r#"{"type":"object","properties":{"id":{"type":"string"}},"required":["id"]}"#,
        true,
    );
    fixture.write("readme.md", "no catalog here\n");
    let mut engine = Weavatrix::open(&fixture.root).unwrap();
    let report = tools::call(
        &mut engine,
        "agent_change_impact",
        json!({"before": "catalogs/before.json", "after": "readme.md"}),
    )
    .unwrap();
    assert_eq!(
        report["changes"][0]["change"],
        "unconfirmed",
        "{}",
        dump(&report)
    );
}

#[test]
fn foreign_package_skill_is_not_a_proven_consumer() {
    let fixture = Fixture::empty();
    write_named(
        &fixture,
        "pkg-a/catalogs/before.json",
        "search",
        r#"{"type":"object","properties":{"q":{"type":"string"}}}"#,
        true,
    );
    write_named(
        &fixture,
        "pkg-a/catalogs/after.json",
        "search",
        r#"{"type":"object","properties":{"q":{"type":"string"}}}"#,
        true,
    );
    fixture.write(
        "pkg-a/skills/alpha/SKILL.md",
        "---\nname: alpha\nallowed-tools: search\n---\nA\n",
    );
    fixture.write(
        "pkg-b/skills/beta/SKILL.md",
        "---\nname: beta\nallowed-tools: search\n---\nB\n",
    );
    let mut engine = Weavatrix::open(&fixture.root).unwrap();
    let report = tools::call(
        &mut engine,
        "agent_change_impact",
        json!({
            "before": "pkg-a/catalogs/before.json",
            "after": "pkg-a/catalogs/after.json"
        }),
    )
    .unwrap();
    let consumers = report["consumers"].as_array().unwrap();
    assert!(
        consumers.iter().any(|item| item["label"] == "alpha"),
        "{}",
        dump(&report)
    );
    assert!(
        consumers.iter().all(|item| item["label"] != "beta"),
        "{}",
        dump(&report)
    );
}

#[test]
fn observation_keeps_event_time_and_does_not_promote_success() {
    let fixture = Fixture::empty();
    fixture.write(
        "catalogs/events.json",
        r#"{
          "$schema": "https://weavatrix.dev/schemas/agent-observation/1.json",
          "events": [
            {
              "id": "inv-1",
              "producer": "granttap",
              "kind": "invocation",
              "target": "lookup",
              "result": "success",
              "event_time": 42,
              "sequence": 7,
              "boot_epoch": 3,
              "phase": "request",
              "evidence": "obs-1"
            }
          ]
        }"#,
    );
    let engine = Weavatrix::open(&fixture.root).unwrap();
    let labels = engine
        .state()
        .graph()
        .nodes()
        .iter()
        .map(|node| node.label.as_str())
        .collect::<Vec<_>>();
    assert!(labels.contains(&"event_time:42"), "{labels:?}");
    assert!(labels.contains(&"sequence:7"), "{labels:?}");
    assert!(labels.contains(&"phase:request"), "{labels:?}");
    assert!(labels.contains(&"result_is_not_effect"), "{labels:?}");
}

#[test]
fn supported_subset_proofs_follow_request_acceptance() {
    assert_eq!(
        impact_compat(
            r#"{"type":"object","properties":{"flag":{"type":"boolean"}}}"#,
            r#"{"type":"object","properties":{"flag":{"type":"boolean","enum":[true,false]}}}"#,
        ),
        "proven-compatible"
    );
    assert_eq!(
        impact_compat(
            r#"{"type":"object","properties":{"n":{"type":"integer"}}}"#,
            r#"{"type":"object","properties":{"n":{"type":"number"}}}"#,
        ),
        "proven-compatible"
    );
    assert_eq!(
        impact_compat(
            r#"{"type":"object","properties":{"n":{"type":"number","minimum":0.1}}}"#,
            r#"{"type":"object","properties":{"n":{"type":"number","minimum":0.5}}}"#,
        ),
        "proven-incompatible"
    );
    assert_eq!(
        impact_compat(
            r#"{"type":"object","properties":{"n":{"type":"number","minimum":0}}}"#,
            r#"{"type":"object","properties":{"n":{"type":"number","exclusiveMinimum":0}}}"#,
        ),
        "proven-incompatible"
    );
    assert_eq!(
        impact_compat(
            r#"{"type":"object","properties":{"id":{"type":"string"}}}"#,
            r#"{"type":"object","properties":{"id":{"type":"string","pattern":"^a$"}}}"#,
        ),
        "undetermined"
    );
    assert_eq!(
        impact_compat(
            r#"{"type":"object","properties":{"mode":{"type":"string","enum":["a,b"]}}}"#,
            r#"{"type":"object","properties":{"mode":{"type":"string","enum":["a","b"]}}}"#,
        ),
        "proven-incompatible"
    );
}

fn impact_compat(before: &str, after: &str) -> String {
    let fixture = Fixture::empty();
    write_schema(&fixture, "catalogs/before.json", before, true);
    write_schema(&fixture, "catalogs/after.json", after, true);
    let mut engine = Weavatrix::open(&fixture.root).unwrap();
    let report = tools::call(
        &mut engine,
        "agent_change_impact",
        json!({"before": "catalogs/before.json", "after": "catalogs/after.json"}),
    )
    .unwrap();
    report["changes"][0]["compatibility"]
        .as_str()
        .unwrap()
        .to_owned()
}

fn write_schema(fixture: &Fixture, path: &str, schema: &str, complete: bool) {
    write_named(fixture, path, "lookup", schema, complete);
}

fn write_named(fixture: &Fixture, path: &str, name: &str, schema: &str, complete: bool) {
    fixture.write(
        path,
        &format!(
            r#"{{
              "$schema": "https://weavatrix.dev/schemas/agent-catalog/1.json",
              "scope": "{name}",
              "complete": {complete},
              "tools": [{{ "name": "{name}", "description": "lookup", "inputSchema": {schema} }}]
            }}"#
        ),
    );
}
