use crate::support::GitFixture;
use blazingly_json::json;
use weavatrix_rust::{Weavatrix, tools};

#[test]
fn default_inventory_is_a_bounded_architecture_overview() {
    let fixture = GitFixture::new();
    fixture.write("package.json", r#"{"name":"demo"}"#);
    fixture.write("src/domain/order.js", "export const order = 1;\n");
    fixture.write(
        "src/infra/db.js",
        "import { order } from '../domain/order.js';\nexport const db = order;\n",
    );
    let mut engine = Weavatrix::open(&fixture.root).unwrap();
    let report = tools::call(&mut engine, "architecture_inventory", json!({})).unwrap();
    assert_eq!(report["detail"], "summary", "{report}");
    assert_eq!(report["components_total"], 2);
    assert!(report["edges_total"].as_u64().unwrap() > 0);
    assert_eq!(report["coupling"][0]["from"], "src/infra");
    assert_eq!(report["coupling"][0]["to"], "src/domain");
    assert!(report.get("edges").is_none(), "{report}");
    assert!(report.get("build_topology").is_none(), "{report}");
    assert!(blazingly_json::to_vec(&report).unwrap().len() < 8_000);
    let full = tools::call(
        &mut engine,
        "architecture_inventory",
        json!({"detail":"full"}),
    )
    .unwrap();
    assert_eq!(full["detail"], "full");
    assert_eq!(report["edges_total"], full["edges_total"]);
    assert_eq!(report["analysis_id"], full["analysis_id"]);
    assert!(full["edges"].as_array().is_some());
}

#[test]
fn quotient_cycle_is_not_claimed_as_symbol_recursion() {
    let fixture = GitFixture::new();
    fixture.write(
        "a/a1.js",
        "import { b } from '../b/b1.js';\nexport const a = b;\n",
    );
    fixture.write("a/a2.js", "export const a2 = 1;\n");
    fixture.write("b/b1.js", "export const b = 1;\n");
    fixture.write(
        "b/b2.js",
        "import { a2 } from '../a/a2.js';\nexport const b2 = a2;\n",
    );
    let mut engine = Weavatrix::open(&fixture.root).unwrap();
    let report = tools::call(
        &mut engine,
        "architecture_inventory",
        json!({"detail":"full"}),
    )
    .unwrap();
    assert_eq!(report["cycles"]["cyclic_scc_total"], 1, "{report}");
    assert_eq!(
        report["cycles"]["classification"],
        "QUOTIENT_UNION_CYCLE_CANDIDATE"
    );
    let cycle = &report["cycles"]["cyclic_scc"][0];
    assert_eq!(report["cycles"]["condensation_nodes"], 1);
    assert_eq!(
        report["cycles"]["condensation_components"][0]["members"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
    assert!(
        !cycle["component_edge_witness"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    assert!(
        cycle["component_edge_witness"][0]["base_edge_indices"]
            .as_array()
            .is_some()
    );
    assert_eq!(cycle["lifting_status"], "LOWER_GRAPH_CYCLE_NOT_PROVEN");
    assert_eq!(
        cycle["configuration_status"],
        "CONDITION_COMPATIBILITY_NOT_PROVEN"
    );
}

#[test]
fn internal_edges_are_connectivity_not_component_self_cycles() {
    let fixture = GitFixture::new();
    fixture.write(
        "src/domain/a.js",
        "import { b } from './b.js';\nexport const a = b;\n",
    );
    fixture.write("src/domain/b.js", "export const b = 1;\n");
    let mut engine = Weavatrix::open(&fixture.root).unwrap();
    let report = tools::call(
        &mut engine,
        "architecture_inventory",
        json!({"detail":"full"}),
    )
    .unwrap();
    assert_eq!(report["cycles"]["cyclic_scc_total"], 0, "{report}");
    assert!(report["internal_connectivity_total"].as_u64().unwrap() > 0);
    assert!(
        report["internal_connectivity"][0]["base_edge_indices"]
            .as_array()
            .is_some()
    );
}

#[test]
fn architecture_inventory_reuses_the_typed_build_model() {
    let fixture = GitFixture::new();
    fixture.write(
        "package.json",
        r#"{"name":"app","scripts":{"build":"tsc"}}"#,
    );
    fixture.write("src/app.js", "export const app = 1;\n");
    let mut engine = Weavatrix::open(&fixture.root).unwrap();
    let report = tools::call(
        &mut engine,
        "architecture_inventory",
        json!({"detail":"full"}),
    )
    .unwrap();
    assert_eq!(
        report["build_topology"]["schema_version"],
        "weavatrix.build-model.v1"
    );
    let member = &report["build_topology"]["workspaces"][0]["members"][0];
    assert_eq!(member["targets"], json!([]));
    assert_eq!(member["tasks"][0]["name"], "build");
}

#[test]
fn pagination_exposes_late_cycles_and_rejects_stale_cursors() {
    let fixture = GitFixture::new();
    for i in 0..205 {
        let next = if i == 204 { 203 } else { i + 1 };
        fixture.write(
            &format!("unit_{i:03}/a.js"),
            &format!("import {{ value as next }} from '../unit_{next:03}/a.js';\nexport const value = next;\n"),
        );
    }
    let mut engine = Weavatrix::open(&fixture.root).unwrap();
    let first = tools::call(
        &mut engine,
        "architecture_inventory",
        json!({"max_results": 40}),
    )
    .unwrap();
    assert_eq!(first["status"], "INCOMPLETE");
    assert!(first["edges_total"].as_u64().unwrap() > 200, "{first}");
    assert!(first["cycles"]["cyclic_scc_total"].as_u64().unwrap() > 0);
    let mut seen = first["edges_returned"].as_u64().unwrap();
    let mut cursor = first["next_edge_cursor"].as_str().unwrap().to_owned();
    loop {
        let page = tools::call(
            &mut engine,
            "architecture_inventory",
            json!({"max_results": 40, "edge_cursor": cursor}),
        )
        .unwrap();
        seen += page["edges_returned"].as_u64().unwrap();
        if let Some(next) = page["next_edge_cursor"].as_str() {
            cursor = next.to_owned();
        } else {
            assert_eq!(page["status"], "COMPLETE");
            break;
        }
    }
    assert_eq!(seen, first["edges_total"].as_u64().unwrap());
    fixture.write("unit_204/a.js", "export const value = 1;\n");
    let mut changed = Weavatrix::open(&fixture.root).unwrap();
    assert!(
        tools::call(
            &mut changed,
            "architecture_inventory",
            json!({"edge_cursor": first["next_edge_cursor"]})
        )
        .is_err()
    );
}

#[test]
fn tiny_token_budget_preserves_identity_totals_and_continuation() {
    let fixture = GitFixture::new();
    for index in 0..20 {
        let next = index + 1;
        fixture.write(
            &format!("part_{index}/a.js"),
            &format!("import {{ value }} from '../part_{next}/a.js';\nexport {{ value }};\n"),
        );
    }
    fixture.write("part_20/a.js", "export const value = 1;\n");
    let mut engine = Weavatrix::open(&fixture.root).unwrap();
    let report = tools::call(
        &mut engine,
        "architecture_inventory",
        json!({"max_results": 100, "token_budget": 250}),
    )
    .unwrap();
    assert_eq!(report["token_budget"]["applied"], true);
    assert_eq!(report["status"], "INCOMPLETE");
    assert!(report["analysis_id"].as_str().is_some());
    assert_eq!(
        report["edges_returned"].as_u64().unwrap(),
        report["edges"].as_array().unwrap().len() as u64
    );
    assert!(report["edges_total"].as_u64().unwrap() >= report["edges_returned"].as_u64().unwrap());
    assert!(report["next_edge_cursor"].as_str().is_some());
}
