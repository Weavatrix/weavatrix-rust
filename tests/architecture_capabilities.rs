//! How one declared capability resolves against the endpoints a revision
//! exposes. Scope and boundedness live in `architecture_capability_scope`.

#[allow(dead_code)]
mod capability_contract;
#[allow(dead_code)]
mod support;

use blazingly_json::json;
use capability_contract::{call, codes, contract, repository, unmapped};
use weavatrix_rust::{Weavatrix, tools};

#[test]
fn a_resolved_claim_passes_and_names_the_component_backing_it() {
    let fixture = repository();
    fixture.write(
        ".weavatrix/architecture.json",
        &contract(
            r#"[
              {"id": "users", "name": "Users",
               "endpoints": ["GET /users/{id}", "POST /users"],
               "components": ["api"]},
              {"id": "audit", "name": "Audit", "endpoints": ["GET /audit"]}
            ]"#,
        ),
    );
    let report = call(&fixture);

    assert_eq!(report["state"], "PASS");
    assert_eq!(report["enforceable"], true);
    assert_eq!(report["declared"], 2);
    assert_eq!(report["served"][0]["id"], "users");
    assert_eq!(report["served"][0]["endpoints"][0]["components"][0], "api");
    assert_eq!(unmapped(&report), Vec::<String>::new());
}

#[test]
fn a_claim_this_revision_cannot_serve_is_orphaned() {
    let fixture = repository();
    fixture.write(
        ".weavatrix/architecture.json",
        &contract(
            r#"[
              {"id": "users", "endpoints": ["GET /users/{id}", "DELETE /users/{id}"]},
              {"id": "audit", "endpoints": ["GET /audit"]}
            ]"#,
        ),
    );
    let report = call(&fixture);

    assert_eq!(report["state"], "BLOCKED");
    assert_eq!(codes(&report, "orphaned"), ["users"]);
    assert_eq!(
        report["orphaned"][0]["evidence"]["endpoints"],
        json!(["DELETE /users/{id}"]),
        "the finding must name the endpoint that resolved to nothing"
    );
    assert_eq!(report["orphaned"][0]["code"], "capability.orphaned");
    assert_eq!(unmapped(&report), ["POST /users"]);
}

#[test]
fn a_claim_served_from_another_component_has_drifted() {
    let fixture = repository();
    fixture.write(
        ".weavatrix/architecture.json",
        &contract(
            r#"[
              {"id": "users", "endpoints": ["GET /users/{id}", "POST /users"],
               "components": ["api"]},
              {"id": "audit", "endpoints": ["GET /audit"], "components": ["api"]}
            ]"#,
        ),
    );
    let report = call(&fixture);

    assert_eq!(report["state"], "BLOCKED");
    assert_eq!(codes(&report, "drifted"), ["audit"]);
    let evidence = &report["drifted"][0]["evidence"]["endpoints"][0];
    assert_eq!(evidence["endpoint"], "GET /audit");
    assert_eq!(evidence["declared_in"], json!(["admin"]));
    assert_eq!(evidence["contract_expects"], json!(["api"]));
}

#[test]
fn findings_are_stable_across_repeated_verification() {
    let fixture = repository();
    fixture.write(
        ".weavatrix/architecture.json",
        &contract(r#"[{"id": "users", "endpoints": ["DELETE /users/{id}"]}]"#),
    );

    let first = call(&fixture);
    let second = call(&fixture);
    assert_eq!(
        first["orphaned"][0]["fingerprint"],
        second["orphaned"][0]["fingerprint"]
    );
    assert!(first["orphaned"][0]["fingerprint"].is_string());
}

#[test]
fn an_endpoint_no_capability_claims_is_reported_without_blocking() {
    let fixture = repository();
    fixture.write(
        ".weavatrix/architecture.json",
        &contract(r#"[{"id": "users", "endpoints": ["GET /users/{id}"]}]"#),
    );
    let report = call(&fixture);

    assert_eq!(
        report["state"], "PASS",
        "unclaimed evidence is a report, not a violation"
    );
    assert_eq!(unmapped(&report), ["GET /audit", "POST /users"]);
}

#[test]
fn a_capability_the_engine_cannot_resolve_is_rejected_rather_than_passed() {
    for (capabilities, expected) in [
        (
            r#"[{"name": "no id", "endpoints": ["GET /users/{id}"]}]"#,
            "without an `id`",
        ),
        (r#"[{"id": "users"}]"#, "no `endpoints`"),
        (
            r#"[{"id": "users", "endpoints": ["GET /users/{id}"]},
                {"id": "users", "endpoints": ["POST /users"]}]"#,
            "more than once",
        ),
    ] {
        let fixture = repository();
        fixture.write(".weavatrix/architecture.json", &contract(capabilities));
        let mut engine = Weavatrix::open(&fixture.root).unwrap();
        let error = tools::call(&mut engine, "verify_capabilities", json!({}))
            .expect_err("an unresolvable capability must fail closed");
        assert!(error.contains(expected), "got {error}");
    }
}
