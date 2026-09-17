#[allow(dead_code)]
mod agent_support;

use agent_support::{Fixture, dump};
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
