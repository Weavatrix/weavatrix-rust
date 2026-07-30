#![cfg(all(
    feature = "clone",
    feature = "git",
    feature = "lang-rust",
    feature = "memory",
    feature = "search",
    feature = "semantic",
    feature = "vector"
))]

mod support;

use blazingly_json::{Value, json};
use support::GitFixture;
use weavatrix_rust::{Weavatrix, tools};

#[test]
fn offline_audit_exposes_no_vulnerability_or_malware_surface() {
    let fixture = GitFixture::new();
    fixture.write(
        "src/main.js",
        "export function main() { return 'offline health only'; }\n",
    );
    fixture.commit("offline");
    let mut engine = Weavatrix::open(&fixture.root).unwrap();

    let definition = tools::catalog()
        .into_iter()
        .find(|tool| tool.name == "run_audit")
        .expect("run_audit stays available for offline repository health");
    assert_no_security_surface(&definition.input_schema);

    let report = tools::call(
        &mut engine,
        "run_audit",
        json!({"max_findings": 20, "include_malware_scan": true}),
    )
    .expect("legacy security arguments must not re-enable an offline scanner");
    assert_no_security_surface(&report);
}

fn assert_no_security_surface(value: &Value) {
    const FORBIDDEN: &[&str] = &["malware", "vulnerab", "advisory", "osv"];
    match value {
        Value::Object(entries) => {
            for (key, nested) in entries {
                let normalized = key.to_ascii_lowercase();
                assert!(
                    !FORBIDDEN.iter().any(|term| normalized.contains(term)),
                    "offline tool surface contains forbidden security key {key}"
                );
                assert_no_security_surface(nested);
            }
        }
        Value::Array(items) => {
            for item in items {
                assert_no_security_surface(item);
            }
        }
        _ => {}
    }
}
