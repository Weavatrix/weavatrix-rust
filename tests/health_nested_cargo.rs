mod language_fixture;

use blazingly_json::json;
use language_fixture::Fixture;
use weavatrix_rust::{Weavatrix, tools};

#[test]
#[cfg(feature = "lang-rust")]
fn audit_counts_local_path_crate_imports_from_nested_packages() {
    let fixture = Fixture::new();
    fixture.write(
        "Cargo.toml",
        "[package]\nname = \"fixture-core\"\nversion = \"0.1.0\"\n",
    );
    fixture.write("src/lib.rs", "pub const VERSION: &str = \"1\";\n");
    fixture.write(
        "tools/standalone-cli/Cargo.toml",
        "[package]\nname = \"fixture-standalone-cli\"\nversion = \"0.0.0\"\n\n[workspace]\n\n[dependencies]\nfixture-core = { path = \"../..\" }\n",
    );
    fixture.write(
        "tools/standalone-cli/src/main.rs",
        "use fixture_core::VERSION;\nfn main() { println!(\"{}\", VERSION); }\n",
    );

    let mut engine = Weavatrix::open(&fixture.root).unwrap();
    let audit = tools::call(&mut engine, "run_audit", json!({})).unwrap();
    let findings = audit["dependency_report"]["findings"].as_array().unwrap();

    assert!(
        !findings.iter().any(|finding| {
            finding["rule"] == "dependency.unused_declaration"
                && finding["manifest"] == "tools/standalone-cli/Cargo.toml"
                && finding["package"] == "fixture-core"
        }),
        "a nested package's path dependency resolves locally but remains used: {findings:?}"
    );
}

#[test]
#[cfg(feature = "lang-rust")]
fn audit_excludes_named_benchmark_packages_but_not_generic_tools() {
    let fixture = Fixture::new();
    fixture.write(
        "tools/competitor-bench/src/main.rs",
        "fn main() { let _ = Some(1).unwrap(); }\n",
    );
    fixture.write(
        "tools/release-tool/src/main.rs",
        "fn main() { let _ = Some(1).unwrap(); }\n",
    );

    let mut engine = Weavatrix::open(&fixture.root).unwrap();
    let audit = tools::call(&mut engine, "run_audit", json!({})).unwrap();
    let findings = audit["runtime_report"]["findings"].as_array().unwrap();

    assert!(
        !findings.iter().any(|finding| {
            finding["rule"] == "runtime.unchecked_unwrap"
                && finding["file"] == "tools/competitor-bench/src/main.rs"
        }),
        "benchmark harness assertions are not production panic findings: {findings:?}"
    );
    assert!(
        findings.iter().any(|finding| {
            finding["rule"] == "runtime.unchecked_unwrap"
                && finding["file"] == "tools/release-tool/src/main.rs"
        }),
        "generic tools may ship and remain inside the production audit: {findings:?}"
    );
}

#[test]
#[cfg(feature = "lang-rust")]
fn dead_code_starts_from_nested_cargo_default_targets() {
    let fixture = Fixture::new();
    fixture.write(
        "tools/standalone-cli/Cargo.toml",
        "[package]\nname = \"standalone-cli\"\nversion = \"0.0.0\"\n\n[workspace]\n",
    );
    fixture.write(
        "tools/standalone-cli/src/main.rs",
        "mod runner;\nfn main() { runner::run(); }\n",
    );
    fixture.write("tools/standalone-cli/src/runner.rs", "pub fn run() {}\n");
    fixture.write(
        "tools/standalone-cli/src/orphan.rs",
        "pub fn forgotten() {}\n",
    );

    let mut engine = Weavatrix::open(&fixture.root).unwrap();
    let report = tools::call(&mut engine, "find_dead_code", json!({"top_n": 50})).unwrap();
    let candidates = report["candidates"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|item| item["node"]["id"].as_str())
        .collect::<Vec<_>>();

    for reachable in [
        "file:tools/standalone-cli/src/main.rs",
        "file:tools/standalone-cli/src/runner.rs",
    ] {
        assert!(
            !candidates.contains(&reachable),
            "{reachable} is reachable from the nested Cargo binary: {candidates:?}"
        );
    }
    assert!(
        candidates.contains(&"file:tools/standalone-cli/src/orphan.rs"),
        "an unreferenced nested source remains reviewable: {candidates:?}"
    );
}
