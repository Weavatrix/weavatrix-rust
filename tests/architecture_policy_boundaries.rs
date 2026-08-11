#[allow(dead_code)]
mod support;

use blazingly_json::{Value, json};
use support::GitFixture;
use weavatrix_rust::{Weavatrix, tools};

#[test]
fn allow_only_blocks_known_and_unmapped_targets_outside_the_allow_list() {
    let fixture = GitFixture::new();
    fixture.write(
        "src/ui/entry.js",
        "import './helper.js';\n\
         import '../service/orders.js';\n\
         import '../infra/database.js';\n\
         import '../legacy/bridge.js';\n",
    );
    fixture.write("src/ui/helper.js", "export const helper = true;\n");
    fixture.write("src/service/orders.js", "export const orders = true;\n");
    fixture.write("src/infra/database.js", "export const database = true;\n");
    fixture.write("src/legacy/bridge.js", "export const bridge = true;\n");
    write_contract(
        &fixture,
        &json!({
            "components": [
                {"id": "ui", "paths": ["src/ui"]},
                {"id": "service", "paths": ["src/service"]},
                {"id": "infra", "paths": ["src/infra"]}
            ],
            "dependencyRules": [{
                "id": "ui-depends-only-on-service",
                "action": "allow_only",
                "from": ["ui"],
                "to": ["service"],
                "kinds": ["imports"]
            }],
            "ratchet": {"baseline": {"fingerprints": []}}
        }),
    );

    let report = verify(&fixture);
    assert_eq!(report["state"], "BLOCKED");
    assert_eq!(report["new"].as_array().map(Vec::len), Some(2));
    let targets = report["new"]
        .as_array()
        .unwrap()
        .iter()
        .map(|violation| violation["evidence"]["target_component"].clone())
        .collect::<Vec<_>>();
    assert!(targets.contains(&json!("infra")), "report: {report:?}");
    assert!(targets.contains(&json!("(unmapped)")), "report: {report:?}");
    assert!(report["new"].as_array().unwrap().iter().all(|violation| {
        violation["rule"]["id"] == "ui-depends-only-on-service"
            && violation["evidence"]["kind"] == "dependency_outside_allow_list"
    }));
}

#[test]
fn unresolved_policy_is_scoped_and_stable_across_line_shifts() {
    let fixture = GitFixture::new();
    fixture.write("src/app/entry.js", "import './missing.js';\n");
    fixture.write("src/other/entry.js", "import './also-missing.js';\n");
    write_contract(
        &fixture,
        &json!({
            "components": [
                {"id": "app", "paths": ["src/app"]},
                {"id": "other", "paths": ["src/other"]}
            ],
            "dependencyRules": [{
                "id": "app-imports-must-resolve",
                "action": "forbid",
                "from": ["app"],
                "kinds": ["unresolved"]
            }],
            "ratchet": {"baseline": {"fingerprints": []}}
        }),
    );

    let first = verify(&fixture);
    assert_eq!(first["new"].as_array().map(Vec::len), Some(1));
    assert_eq!(first["new"][0]["source"]["file"], "src/app/entry.js");
    assert_eq!(
        first["new"][0]["evidence"]["diagnostic"]["code"],
        "import.unresolved"
    );
    let fingerprint = first["new"][0]["fingerprint"].clone();

    fixture.write(
        "src/app/entry.js",
        "// line inserted before the import\nimport './missing.js';\n",
    );
    let shifted = verify(&fixture);
    assert_eq!(shifted["new"].as_array().map(Vec::len), Some(1));
    assert_eq!(shifted["new"][0]["fingerprint"], fingerprint);
}

#[test]
fn unsupported_action_and_kind_combinations_fail_closed() {
    let fixture = GitFixture::new();
    fixture.write("src/app/entry.js", "export const entry = true;\n");
    for (rule, expected) in [
        (
            json!({
                "id": "transitive-allow-list",
                "action": "allow_only",
                "reachability": "transitive",
                "from": ["app"],
                "to": [],
                "kinds": ["imports"]
            }),
            "allow_only",
        ),
        (
            json!({
                "id": "transitive-unresolved",
                "action": "forbid",
                "reachability": "transitive",
                "from": ["app"],
                "kinds": ["unresolved"]
            }),
            "unresolved",
        ),
        (
            json!({
                "id": "require-unresolved",
                "action": "require",
                "from": ["app"],
                "kinds": ["unresolved"]
            }),
            "unresolved",
        ),
    ] {
        write_contract(
            &fixture,
            &json!({
                "components": [{"id": "app", "paths": ["src/app"]}],
                "dependencyRules": [rule],
                "ratchet": {"baseline": {"fingerprints": []}}
            }),
        );
        let mut engine = Weavatrix::open(&fixture.root).unwrap();
        let error = tools::call(&mut engine, "verify_architecture", json!({}))
            .expect_err("unsupported policy combination must not produce PASS");
        assert!(error.contains(expected), "unexpected error: {error}");
    }
}

fn write_contract(fixture: &GitFixture, contract: &Value) {
    fixture.write(
        ".weavatrix/architecture.json",
        &blazingly_json::to_string_pretty(contract).unwrap(),
    );
}

fn verify(fixture: &GitFixture) -> Value {
    let mut engine = Weavatrix::open(&fixture.root).unwrap();
    tools::call(&mut engine, "verify_architecture", json!({})).unwrap()
}
