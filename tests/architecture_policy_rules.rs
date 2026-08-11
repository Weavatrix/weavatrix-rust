#[allow(dead_code)]
mod support;

use blazingly_json::{Value, json};
use support::GitFixture;
use weavatrix_rust::{Weavatrix, tools};

#[test]
fn transitive_forbid_returns_the_shortest_component_path() {
    let fixture = GitFixture::new();
    fixture.write(
        "src/ui/entry.js",
        "import '../service/orders.js';\nimport '../service/long-route.js';\n",
    );
    fixture.write("src/service/orders.js", "import '../infra/database.js';\n");
    fixture.write(
        "src/service/long-route.js",
        "import '../domain/order.js';\n",
    );
    fixture.write("src/domain/order.js", "import '../infra/database.js';\n");
    fixture.write("src/infra/database.js", "export const database = true;\n");

    write_contract(
        &fixture,
        &json!({
            "components": components(),
            "dependencyRules": [{
                "id": "ui-cannot-reach-infra",
                "action": "forbid",
                "from": ["ui"],
                "to": ["infra"],
                "kinds": ["imports"]
            }],
            "ratchet": {"baseline": {"fingerprints": []}}
        }),
    );
    assert_eq!(verify(&fixture)["state"], "PASS");

    write_contract(
        &fixture,
        &json!({
            "components": components(),
            "dependencyRules": [{
                "id": "ui-cannot-reach-infra",
                "action": "forbid",
                "reachability": "transitive",
                "from": ["ui"],
                "to": ["infra"],
                "kinds": ["imports"]
            }],
            "ratchet": {"baseline": {"fingerprints": []}}
        }),
    );
    let report = verify(&fixture);
    assert_eq!(report["state"], "BLOCKED");
    assert_eq!(report["new"].as_array().map(Vec::len), Some(1));
    assert_eq!(
        report["new"][0]["evidence"]["path_files"],
        json!([
            "src/ui/entry.js",
            "src/service/orders.js",
            "src/infra/database.js"
        ])
    );
}

#[test]
fn transitive_require_reports_only_source_files_without_a_path() {
    let fixture = GitFixture::new();
    fixture.write(
        "src/controllers/orders.js",
        "import '../service/orders.js';\n",
    );
    fixture.write("src/service/orders.js", "import '../auth/middleware.js';\n");
    fixture.write(
        "src/controllers/health.js",
        "import '../service/health.js';\n",
    );
    fixture.write("src/service/health.js", "export const health = true;\n");
    fixture.write("src/auth/middleware.js", "export const auth = true;\n");
    write_contract(
        &fixture,
        &json!({
            "components": components(),
            "dependencyRules": [{
                "id": "controllers-require-auth",
                "action": "require",
                "reachability": "transitive",
                "from": ["controllers"],
                "to": ["auth"],
                "kinds": ["imports"]
            }],
            "ratchet": {"baseline": {"fingerprints": []}}
        }),
    );

    let report = verify(&fixture);
    assert_eq!(report["state"], "BLOCKED");
    assert_eq!(report["new"].as_array().map(Vec::len), Some(1));
    assert_eq!(report["new"][0]["rule"]["id"], "controllers-require-auth");
    assert_eq!(
        report["new"][0]["source"]["file"],
        "src/controllers/health.js"
    );
}

#[test]
fn direct_require_accepts_one_hop_and_reports_missing_sources() {
    let fixture = GitFixture::new();
    fixture.write(
        "src/controllers/orders.js",
        "import '../auth/middleware.js';\n",
    );
    fixture.write("src/controllers/health.js", "export const health = true;\n");
    fixture.write("src/auth/middleware.js", "export const auth = true;\n");
    write_contract(
        &fixture,
        &json!({
            "components": components(),
            "dependencyRules": [{
                "id": "controllers-require-auth",
                "action": "require",
                "from": ["controllers"],
                "to": ["auth"],
                "kinds": ["imports"]
            }],
            "ratchet": {"baseline": {"fingerprints": []}}
        }),
    );

    let report = verify(&fixture);
    assert_eq!(report["new"].as_array().map(Vec::len), Some(1));
    assert_eq!(
        report["new"][0]["source"]["file"],
        "src/controllers/health.js"
    );
}

#[test]
fn unsupported_actions_and_reachability_fail_closed() {
    let fixture = GitFixture::new();
    fixture.write("src/ui/entry.js", "export const entry = true;\n");
    for (field, expected) in [
        (("action", "allow_only"), "allow_only"),
        (("reachability", "recursive"), "recursive"),
    ] {
        let mut rule = json!({
            "id": "unsupported",
            "action": "forbid",
            "from": ["ui"],
            "to": ["infra"],
            "kinds": ["imports"]
        });
        rule[field.0] = json!(field.1);
        write_contract(
            &fixture,
            &json!({
                "components": components(),
                "dependencyRules": [rule],
                "ratchet": {"baseline": {"fingerprints": []}}
            }),
        );
        let mut engine = Weavatrix::open(&fixture.root).unwrap();
        let error = tools::call(&mut engine, "verify_architecture", json!({}))
            .expect_err("unsupported policy vocabulary must not produce PASS");
        assert!(error.contains(expected), "unexpected error: {error}");
    }
}

fn components() -> Value {
    json!([
        {"id": "ui", "paths": ["src/ui"]},
        {"id": "controllers", "paths": ["src/controllers"]},
        {"id": "service", "paths": ["src/service"]},
        {"id": "domain", "paths": ["src/domain"]},
        {"id": "auth", "paths": ["src/auth"]},
        {"id": "infra", "paths": ["src/infra"]}
    ])
}

fn write_contract(fixture: &GitFixture, contract: &Value) {
    fixture.write(
        ".weavatrix/architecture.json",
        &blazingly_json::to_string_pretty(&contract).unwrap(),
    );
}

fn verify(fixture: &GitFixture) -> Value {
    let mut engine = Weavatrix::open(&fixture.root).unwrap();
    tools::call(&mut engine, "verify_architecture", json!({})).unwrap()
}
