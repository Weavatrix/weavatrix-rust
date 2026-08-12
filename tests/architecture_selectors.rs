#[allow(dead_code)]
mod support;

use blazingly_json::{Value, json};
use std::process::Command;
use support::GitFixture;
use weavatrix_rust::{Weavatrix, tools};

fn selector_fixture(rules: &Value) -> GitFixture {
    let fixture = GitFixture::new();
    fixture.write(
        "src/presentation/entry.js",
        "import \"../data/store.js\";\n",
    );
    fixture.write("src/data/store.js", "export const store = true;\n");
    fixture.write(".weavatrix/architecture.json", &contract(rules));
    fixture
}

fn contract(rules: &Value) -> String {
    blazingly_json::to_string_pretty(&json!({
        "architectureContractV": 1,
        "components": [
            {"id": "presentation", "paths": ["src/presentation"]},
            {"id": "data", "paths": ["src/data"]}
        ],
        "dependencyRules": rules,
        "ratchet": {"baseline": {"fingerprints": []}}
    }))
    .unwrap()
}

fn verify(fixture: &GitFixture) -> Value {
    let mut engine = Weavatrix::open(&fixture.root).unwrap();
    tools::call(&mut engine, "verify_architecture", json!({})).unwrap()
}

fn verify_error(fixture: &GitFixture) -> String {
    let mut engine = Weavatrix::open(&fixture.root).unwrap();
    tools::call(&mut engine, "verify_architecture", json!({}))
        .expect_err("an unsupported contract must fail closed")
}

#[test]
fn path_selectors_block_matching_dependencies() {
    let fixture = selector_fixture(&json!([{
        "id": "path-selector",
        "action": "forbid",
        "fromPath": "^src/presentation/",
        "toPath": "^src/data/",
        "kinds": ["imports"]
    }]));

    let first = verify(&fixture);
    let second = verify(&fixture);

    assert_eq!(first["state"], "BLOCKED", "{first}");
    let violation = &first["new"][0];
    assert_eq!(violation["rule"]["id"], "path-selector");
    assert_eq!(violation["category"], "dependency");
    assert!(violation["fingerprint"].as_str().is_some());
    assert_eq!(first["new"], second["new"]);
}

#[test]
fn capturing_selectors_and_exclusions_scope_the_rule() {
    let captured = selector_fixture(&json!([{
        "id": "captured",
        "action": "forbid",
        "fromPath": "^src/([^/]+)/entry\\.js$",
        "toPath": "^src/data/",
        "kinds": ["imports"]
    }]));
    assert_eq!(verify(&captured)["state"], "BLOCKED");

    let excluded = selector_fixture(&json!([{
        "id": "excluded",
        "action": "forbid",
        "fromPath": "^src/",
        "fromPathNot": "^src/presentation/",
        "toPath": "^src/data/",
        "kinds": ["imports"]
    }]));
    let report = verify(&excluded);
    assert_eq!(report["state"], "PASS", "{report}");
    assert_eq!(report["new"], json!([]));
}

#[test]
fn warn_severity_reports_without_blocking() {
    let warned = selector_fixture(&json!([{
        "id": "severity-boundary",
        "action": "forbid",
        "severity": "warn",
        "from": ["presentation"],
        "to": ["data"],
        "kinds": ["imports"]
    }]));

    let report = verify(&warned);
    assert_eq!(report["state"], "PASS", "{report}");
    assert_eq!(report["new"], json!([]));
    assert_eq!(report["warnings"][0]["rule"]["id"], "severity-boundary");

    let blocking = selector_fixture(&json!([{
        "id": "severity-boundary",
        "action": "forbid",
        "severity": "error",
        "from": ["presentation"],
        "to": ["data"],
        "kinds": ["imports"]
    }]));
    assert_eq!(verify(&blocking)["state"], "BLOCKED");
}

#[test]
fn unsupported_rule_shapes_fail_closed() {
    let unknown = selector_fixture(&json!([{
        "id": "unknown-selector",
        "action": "forbid",
        "from": ["presentation"],
        "to": ["data"],
        "kinds": ["imports"],
        "unknownSelectorField": true
    }]));
    assert!(
        verify_error(&unknown).contains("unknown-selector.unknownSelectorField"),
        "unknown rule fields must be named in the rejection"
    );

    let mixed = selector_fixture(&json!([{
        "id": "mixed",
        "action": "forbid",
        "from": ["presentation"],
        "fromPath": "^src/presentation/",
        "toPath": "^src/data/",
        "kinds": ["imports"]
    }]));
    assert!(verify_error(&mixed).contains("components or paths"));

    let pattern = selector_fixture(&json!([{
        "id": "shorthand",
        "action": "forbid",
        "fromPath": "^src/\\d+/",
        "toPath": "^src/data/",
        "kinds": ["imports"]
    }]));
    assert!(verify_error(&pattern).contains("not supported"));

    let severity = selector_fixture(&json!([{
        "id": "info-severity",
        "action": "forbid",
        "severity": "info",
        "from": ["presentation"],
        "to": ["data"],
        "kinds": ["imports"]
    }]));
    assert!(verify_error(&severity).contains("severity"));

    let action = selector_fixture(&json!([{
        "id": "required-path",
        "action": "require",
        "fromPath": "^src/presentation/",
        "toPath": "^src/data/",
        "kinds": ["imports"]
    }]));
    assert!(verify_error(&action).contains("direct forbid"));
}

#[test]
fn group_references_bind_the_target_to_the_captured_source() {
    let fixture = GitFixture::new();
    fixture.write("src/alpha/ui/panel.js", "import \"../db/rows.js\";\n");
    fixture.write("src/alpha/db/rows.js", "export const rows = [];\n");
    fixture.write(
        "src/beta/ui/panel.js",
        "import \"../../alpha/db/rows.js\";\n",
    );
    fixture.write(
        ".weavatrix/architecture.json",
        &contract(&json!([{
            "id": "own-feature-db",
            "action": "forbid",
            "fromPath": "^src/([^/]+)/ui/",
            "toPath": "^src/$1/db/",
            "kinds": ["imports"]
        }])),
    );

    let report = verify(&fixture);
    assert_eq!(report["state"], "BLOCKED", "{report}");
    let new = report["new"].as_array().unwrap();
    assert_eq!(new.len(), 1, "only the same-feature edge matches: {report}");
    assert!(
        report.to_string().contains("src/alpha/ui/panel.js"),
        "{report}"
    );

    let overflow = selector_fixture(&json!([{
        "id": "overflow",
        "action": "forbid",
        "fromPath": "^src/([^/]+)/",
        "toPath": "^src/$2/",
        "kinds": ["imports"]
    }]));
    assert!(verify_error(&overflow).contains("$2"));

    let unreferenced = selector_fixture(&json!([{
        "id": "unreferenced",
        "action": "forbid",
        "fromPathNot": "^vendor/",
        "toPath": "^src/$1/",
        "kinds": ["imports"]
    }]));
    assert!(verify_error(&unreferenced).contains("fromPath"));
}

#[test]
fn the_cli_exit_code_carries_a_blocked_verification() {
    let fixture = selector_fixture(&json!([{
        "id": "path-selector",
        "action": "forbid",
        "fromPath": "^src/presentation/",
        "toPath": "^src/data/",
        "kinds": ["imports"]
    }]));
    let blocked = Command::new(env!("CARGO_BIN_EXE_weavatrix-rust"))
        .args(["tool", "verify_architecture"])
        .arg(&fixture.root)
        .output()
        .expect("standalone CLI must start");
    assert!(!blocked.status.success());
    let stdout = String::from_utf8(blocked.stdout).expect("report must be UTF-8");
    assert!(stdout.contains("\"BLOCKED\""), "{stdout}");

    let passing = GitFixture::new();
    passing.write("src/lib.js", "export const fine = true;\n");
    passing.write(".weavatrix/architecture.json", &contract(&json!([])));
    let clean = Command::new(env!("CARGO_BIN_EXE_weavatrix-rust"))
        .args(["tool", "verify_architecture"])
        .arg(&passing.root)
        .output()
        .expect("standalone CLI must start");
    assert!(
        clean.status.success(),
        "{}",
        String::from_utf8_lossy(&clean.stderr)
    );
}
