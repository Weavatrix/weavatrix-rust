mod support;

use blazingly_json::json;
use support::GitFixture;
use weavatrix_rust::{Weavatrix, tools};

#[test]
fn symbol_ownership_and_in_file_calls_are_not_dependency_cycles() {
    let fixture = GitFixture::new();
    fixture.write(
        "src/engine.ts",
        r"
export class Engine {
  start() { this.stop(); }
  stop() { this.start(); }
}
",
    );
    fixture.commit("single file symbols");

    let mut engine = Weavatrix::open(&fixture.root).unwrap();
    let audit = tools::call(&mut engine, "run_audit", json!({})).unwrap();

    assert_eq!(audit["cycles"], json!([]));
}

#[test]
fn production_runtime_import_cycle_is_actionable() {
    let fixture = GitFixture::new();
    write_import_cycle(&fixture, "src");
    fixture.commit("runtime cycle");

    let mut engine = Weavatrix::open(&fixture.root).unwrap();
    let audit = tools::call(&mut engine, "run_audit", json!({})).unwrap();

    assert_eq!(audit["cycles"], json!([["file:src/a.ts", "file:src/b.ts"]]));
    assert_eq!(audit["status"], "REVIEW");
}

#[test]
fn type_only_imports_do_not_create_runtime_cycles() {
    let fixture = GitFixture::new();
    fixture.write(
        "src/a.ts",
        "import type { B } from './b';\nexport interface A { b: B }\n",
    );
    fixture.write(
        "src/b.ts",
        "import type { A } from './a';\nexport interface B { a: A }\n",
    );
    fixture.commit("type graph");

    let mut engine = Weavatrix::open(&fixture.root).unwrap();
    let audit = tools::call(&mut engine, "run_audit", json!({})).unwrap();

    assert_eq!(audit["cycles"], json!([]));
}

#[test]
fn test_and_classified_cycles_require_explicit_opt_in() {
    let fixture = GitFixture::new();
    write_import_cycle(&fixture, "tests");
    write_import_cycle(&fixture, "scripts");
    fixture.commit("non product cycles");

    let mut engine = Weavatrix::open(&fixture.root).unwrap();
    let production = tools::call(&mut engine, "run_audit", json!({})).unwrap();
    let tests = tools::call(&mut engine, "run_audit", json!({"include_tests": true})).unwrap();
    let classified = tools::call(
        &mut engine,
        "run_audit",
        json!({"include_classified": true}),
    )
    .unwrap();

    assert_eq!(production["cycles"], json!([]));
    assert_eq!(
        tests["cycles"],
        json!([["file:tests/a.ts", "file:tests/b.ts"]])
    );
    assert_eq!(
        classified["cycles"],
        json!([["file:scripts/a.ts", "file:scripts/b.ts"]])
    );
}

#[test]
fn transport_producer_consumer_cycle_is_actionable() {
    let fixture = GitFixture::new();
    fixture.write(
        "src/orders.js",
        "producer.publish('orders');\nconsumer.subscribe('payments');\n",
    );
    fixture.write(
        "src/payments.js",
        "producer.publish('payments');\nconsumer.subscribe('orders');\n",
    );
    fixture.commit("event cycle");

    let mut engine = Weavatrix::open(&fixture.root).unwrap();
    let audit = tools::call(&mut engine, "run_audit", json!({})).unwrap();

    assert_eq!(
        audit["cycles"],
        json!([["file:src/orders.js", "file:src/payments.js"]])
    );
}

#[test]
#[cfg(feature = "git")]
fn debt_uses_the_same_deterministic_cycle_semantics() {
    let fixture = GitFixture::new();
    write_import_cycle(&fixture, "src");
    fixture.commit("baseline cycle");

    let mut engine = Weavatrix::open(&fixture.root).unwrap();
    let first = tools::call(
        &mut engine,
        "run_audit",
        json!({"base_ref": "HEAD", "debt": "all"}),
    )
    .unwrap();
    let second = tools::call(
        &mut engine,
        "run_audit",
        json!({"base_ref": "HEAD", "debt": "all"}),
    )
    .unwrap();

    assert_eq!(first["cycles"], second["cycles"]);
    assert_eq!(first["debt"]["counts"]["new"], 0);
    assert_eq!(first["debt"]["counts"]["existing"], 1);
    assert_eq!(
        first["debt"]["findings"]["existing"][0]["rule"],
        "structure.dependency_cycle"
    );
}

fn write_import_cycle(fixture: &GitFixture, directory: &str) {
    fixture.write(
        &format!("{directory}/a.ts"),
        "import { b } from './b';\nexport function a() { return b(); }\n",
    );
    fixture.write(
        &format!("{directory}/b.ts"),
        "import { a } from './a';\nexport function b() { return a(); }\n",
    );
}
