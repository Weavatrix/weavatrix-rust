mod language_fixture;

use language_fixture::Fixture;
use weavatrix_rust::{Analyzer, EdgeKind, NodeKind};

#[test]
fn extracts_primary_and_optional_language_facts() {
    let fixture = primary_language_fixture();
    let snapshot = Analyzer::default().analyze(&fixture.root).unwrap();
    for language in [
        "go",
        "c",
        "cpp",
        "bash",
        "sql",
        "kubernetes",
        "javascript",
        "typescript",
        "python",
        "java",
        "csharp",
    ] {
        assert!(
            snapshot
                .capabilities
                .iter()
                .any(|capability| capability.id == format!("lang:{language}")),
            "missing {language} capability"
        );
    }
    for (kind, label) in [
        (NodeKind::Endpoint, "ANY /ready"),
        (NodeKind::Endpoint, "GET /items"),
        (NodeKind::Endpoint, "POST /mapped"),
        (NodeKind::Endpoint, "GET /jobs"),
        (NodeKind::Endpoint, "GET /stock"),
        (NodeKind::Endpoint, "GET /health"),
        (NodeKind::Table, "users"),
        (NodeKind::Collection, "users"),
        (NodeKind::Topic, "orders"),
        (NodeKind::KubernetesResource, "Deployment/api"),
    ] {
        assert!(
            snapshot
                .nodes
                .iter()
                .any(|node| node.kind == kind && node.label == label),
            "missing {kind:?} {label}"
        );
    }
    assert!(
        snapshot
            .edges
            .iter()
            .any(|edge| edge.kind == EdgeKind::Writes)
    );
    assert!(
        snapshot
            .edges
            .iter()
            .any(|edge| edge.kind == EdgeKind::Deploys)
    );
    assert!(
        snapshot
            .edges
            .iter()
            .any(|edge| edge.kind == EdgeKind::Publishes)
    );
}

fn primary_language_fixture() -> Fixture {
    let fixture = Fixture::new();
    fixture.write(
        "go/server.go",
        "package server\nimport \"net/http\"\nfunc run() { http.HandleFunc(\"/ready\", ready) }\n",
    );
    fixture.write("c/main.c", "int helper(void) { return 1; }\n");
    fixture.write(
        "cpp/main.cpp",
        "#include <vector>\nint compute() { return helper(); }\n",
    );
    fixture.write("ops/run.sh", "function deploy() {\n  kubectl apply\n}\n");
    fixture.write(
        "db/query.sql",
        "CREATE TABLE users(id bigint);\nSELECT * FROM users;\nUPDATE users SET id=2;\n",
    );
    fixture.write(
        "deploy/app.yaml",
        "apiVersion: apps/v1\nkind: Deployment\nmetadata:\n  name: api\n",
    );
    fixture.write(
        "web/router.js",
        "export function list() { return db.collection(\"users\"); }\nrouter.get(\"/items\", list);\nconst routes = {\n'/mapped': {\nPOST: list,\n},\n};\n",
    );
    fixture.write(
        "web/client.ts",
        "export class Client {}\nfunction publish(){ topic(\"orders\"); }\n",
    );
    fixture.write(
        "automation/app.py",
        "from flask import Flask\n@app.get(\"/jobs\")\ndef jobs():\n    return run()\n",
    );
    fixture.write(
        "warehouse/Controller.java",
        "public class Controller {\n@GetMapping(\"/stock\") public void stock() {}\n}\n",
    );
    fixture.write(
        "service/Controller.cs",
        "public class Controller {\n[HttpGet(\"/health\")] public void Health() {}\n}\n",
    );
    fixture
}
