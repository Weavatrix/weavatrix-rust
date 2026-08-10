//! What counts as the served surface, and what the answer costs to read.
//!
//! The fixtures declare their routes as Rust attributes, so they need the
//! adapter that reads them.
#![cfg(feature = "lang-rust")]

#[allow(dead_code)]
mod capability_contract;
#[allow(dead_code)]
mod support;

use blazingly_json::json;
use capability_contract::{call, contract, repository, unmapped, verify};
use weavatrix_rust::{Weavatrix, tools};

#[test]
fn a_route_declared_only_by_a_test_is_not_part_of_the_served_surface() {
    let fixture = repository();
    fixture.write(
        "api/fixtures.rs",
        "#[cfg(test)]\nmod tests {\n\
         #[get(\"/only-in-tests\", id = \"fixture.read\")]\n\
         async fn fixture() {}\n}\n",
    );
    fixture.write(
        ".weavatrix/architecture.json",
        &contract(
            r#"[{"id": "everything",
                 "endpoints": ["GET /users/{id}", "POST /users", "GET /audit"]}]"#,
        ),
    );

    let production = call(&fixture);
    assert_eq!(production["state"], "PASS");
    assert_eq!(
        unmapped(&production),
        Vec::<String>::new(),
        "a test fixture route is not an unclaimed part of the served surface"
    );

    let with_tests = verify(&fixture, json!({"include_tests": true}));
    assert_eq!(unmapped(&with_tests), ["GET /only-in-tests"]);
}

#[test]
fn unclaimed_evidence_is_trimmed_by_default_and_always_reports_its_total() {
    let fixture = repository();
    fixture.write(
        ".weavatrix/architecture.json",
        &contract(r#"[{"id": "users", "endpoints": ["GET /users/{id}"]}]"#),
    );

    let bounded = verify(&fixture, json!({"max_results": 1}));
    assert_eq!(bounded["unmapped"].as_array().unwrap().len(), 1);
    assert_eq!(
        bounded["unmapped_total"], 2,
        "a trimmed answer must still say how much it trimmed from"
    );

    let unbounded = verify(&fixture, json!({"max_results": 0}));
    assert_eq!(unbounded["unmapped"].as_array().unwrap().len(), 2);
    assert_eq!(unbounded["unmapped_total"], 2);

    let mut engine = Weavatrix::open(&fixture.root).unwrap();
    let rejected = tools::call(
        &mut engine,
        "verify_capabilities",
        json!({"max_results": "all"}),
    )
    .expect_err("an argument the operation cannot apply must be refused");
    assert!(rejected.contains("max_results must be a non-negative integer"));
}

#[test]
fn a_budget_this_operation_cannot_apply_is_reported_rather_than_refused() {
    let fixture = repository();
    fixture.write(
        ".weavatrix/architecture.json",
        &contract(r#"[{"id": "users", "endpoints": ["GET /users/{id}"]}]"#),
    );
    let report = verify(&fixture, json!({"token_budget": 500}));

    assert_eq!(report["state"], "PASS");
    assert_eq!(report["token_budget"]["applied"], false);
}

#[test]
fn a_contract_without_the_section_reports_the_surface_it_would_declare() {
    let fixture = repository();
    fixture.write(
        ".weavatrix/architecture.json",
        &contract("[]").replace("\"capabilities\": [],", ""),
    );
    let report = call(&fixture);

    assert_eq!(report["state"], "NOT_DECLARED");
    assert_eq!(report["enforceable"], false);
    assert_eq!(
        unmapped(&report),
        ["GET /audit", "GET /users/{id}", "POST /users"],
        "everything the repository exposes is unclaimed until something claims it"
    );
    assert_eq!(
        report["starter"]["capabilities"][0]["endpoints"][0],
        "GET /audit"
    );
    assert_eq!(report["write"], "NONE");
}

#[test]
fn no_contract_at_all_stays_unenforceable_and_offers_a_starter() {
    let report = call(&repository());

    assert_eq!(report["state"], "NOT_CONFIGURED");
    assert_eq!(report["enforceable"], false);
    assert_eq!(report["write"], "NONE");
    assert_eq!(
        report["starter"]["capabilities"][0]["endpoints"][0], "GET /audit",
        "the derived starter is the identity mapping of the exposed surface"
    );
    assert_eq!(
        report["starter"]["capabilities"][0]["files"][0],
        "admin/audit.rs"
    );
}
