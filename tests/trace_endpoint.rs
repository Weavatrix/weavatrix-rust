mod tool_fixture;

use blazingly_json::json;
use tool_fixture::Fixture;
use weavatrix_rust::{Weavatrix, tools};

fn api_fixture() -> Fixture {
    let fixture = Fixture::new();
    fixture.write(
        "src/server.js",
        "import { requireAuth } from './middleware.js';\n\
         import { createOrder } from './orders.js';\n\
         function health() { return 'ok'; }\n\
         function unusedHelper() { return 0; }\n\
         router.get('/api/health', health);\n\
         router.post('/api/orders', requireAuth, createOrder);\n",
    );
    fixture.write(
        "src/middleware.js",
        "export function requireAuth(req) {\n  return Boolean(req.user);\n}\n",
    );
    fixture.write(
        "src/orders.js",
        "import { requireAuth } from './middleware.js';\n\
         import { saveOrder } from './db.js';\n\
         export function createOrder(req) {\n\
           requireAuth(req);\n\
           return saveOrder(req.body);\n\
         }\n",
    );
    fixture.write(
        "src/db.js",
        "export function saveOrder(body) { return body; }\n",
    );
    fixture
}

fn labels(report: &blazingly_json::Value) -> Vec<&str> {
    report["nodes"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|node| node["label"].as_str())
        .collect()
}

#[test]
fn exact_path_rejects_suffix_shortcut() {
    let fixture = api_fixture();
    let mut engine = Weavatrix::open(&fixture.root).unwrap();
    let missed = tools::call(
        &mut engine,
        "trace_endpoint",
        json!({"path": "/orders", "method": "POST"}),
    )
    .unwrap();
    assert_eq!(missed["state"], "NOT_FOUND", "{missed}");

    let matched = tools::call(
        &mut engine,
        "trace_endpoint",
        json!({"path": "/api/orders", "method": "POST"}),
    )
    .unwrap();
    assert_eq!(matched["state"], "COMPLETE", "{matched}");
    assert_eq!(matched["endpoint"]["label"], "POST /api/orders");
}

#[test]
fn exact_method_rejects_prefix_shortcut() {
    let fixture = api_fixture();
    let mut engine = Weavatrix::open(&fixture.root).unwrap();
    let missed = tools::call(
        &mut engine,
        "trace_endpoint",
        json!({"path": "/api/orders", "method": "PO"}),
    )
    .unwrap();
    assert_eq!(missed["state"], "NOT_FOUND", "{missed}");
}

#[test]
fn suffix_match_is_explicit_and_can_be_ambiguous() {
    let fixture = api_fixture();
    fixture.write(
        "src/legacy.js",
        "function list() {}\nrouter.get('/v1/orders', list);\n",
    );
    let mut engine = Weavatrix::open(&fixture.root).unwrap();
    let ambiguous = tools::call(
        &mut engine,
        "trace_endpoint",
        json!({"path": "/orders", "match": "suffix"}),
    )
    .unwrap();
    assert_eq!(ambiguous["state"], "AMBIGUOUS", "{ambiguous}");
    assert!(
        ambiguous["candidates"]
            .as_array()
            .is_some_and(|items| items.len() >= 2),
        "{ambiguous}"
    );
}

#[test]
fn handler_file_filters_and_wrong_hint_is_not_found() {
    let fixture = Fixture::new();
    fixture.write(
        "src/a.js",
        "export function createOrder() { return 1; }\nrouter.post('/api/orders', createOrder);\n",
    );
    fixture.write(
        "src/b.js",
        "export function createOrder() { return 2; }\nrouter.post('/api/orders', createOrder);\n",
    );
    let mut engine = Weavatrix::open(&fixture.root).unwrap();

    let ambiguous = tools::call(
        &mut engine,
        "trace_endpoint",
        json!({"path": "/api/orders", "method": "POST"}),
    )
    .unwrap();
    assert_eq!(ambiguous["state"], "AMBIGUOUS", "{ambiguous}");

    let resolved = tools::call(
        &mut engine,
        "trace_endpoint",
        json!({"path": "/api/orders", "method": "POST", "handler_file": "src/b.js"}),
    )
    .unwrap();
    assert_eq!(resolved["state"], "COMPLETE", "{resolved}");
    let resolved_labels = labels(&resolved);
    assert!(
        resolved_labels
            .iter()
            .any(|label| label.contains("createOrder"))
            || resolved["nodes"]
                .as_array()
                .into_iter()
                .flatten()
                .any(|node| node["id"]
                    .as_str()
                    .is_some_and(|id| id.contains("src/b.js"))),
        "handler from src/b.js must be present: {resolved}"
    );

    let missed = tools::call(
        &mut engine,
        "trace_endpoint",
        json!({
            "path": "/api/orders",
            "method": "POST",
            "handler_file": "src/does-not-exist.ts"
        }),
    )
    .unwrap();
    assert_eq!(missed["state"], "NOT_FOUND", "{missed}");
}

#[test]
fn post_trace_prefers_execution_path_over_neighborhood() {
    let fixture = api_fixture();
    let mut engine = Weavatrix::open(&fixture.root).unwrap();
    let report = tools::call(
        &mut engine,
        "trace_endpoint",
        json!({"path": "/api/orders", "method": "POST", "max_depth": 4, "max_nodes": 20}),
    )
    .unwrap();
    assert_eq!(report["state"], "COMPLETE", "{report}");
    let node_labels = labels(&report);
    assert!(
        !node_labels.contains(&"GET /api/health"),
        "neighboring GET must not appear: {report}"
    );
    assert!(
        !report["nodes"]
            .as_array()
            .into_iter()
            .flatten()
            .any(|node| {
                node["kind"].as_str() == Some("repository")
                    || node["kind"].as_str() == Some("package")
            }),
        "repository/package must not crowd the path: {report}"
    );
    assert!(
        !node_labels
            .iter()
            .any(|label| label.contains("unusedHelper")),
        "unused helper must stay off the execution path: {report}"
    );
    assert!(
        report["nodes"]
            .as_array()
            .into_iter()
            .flatten()
            .any(|node| {
                node["label"].as_str().is_some_and(|label| {
                    label.contains("createOrder") || label.contains("requireAuth")
                }) || node["id"]
                    .as_str()
                    .is_some_and(|id| id.contains("orders.js") || id.contains("middleware.js"))
            }),
        "handler or middleware evidence must remain: {report}"
    );
}

#[test]
fn bounded_trace_keeps_handler_and_marks_truncation() {
    let fixture = api_fixture();
    let mut engine = Weavatrix::open(&fixture.root).unwrap();
    let report = tools::call(
        &mut engine,
        "trace_endpoint",
        json!({"path": "/api/orders", "method": "POST", "max_nodes": 3}),
    )
    .unwrap();
    assert_eq!(report["state"], "COMPLETE", "{report}");
    assert_eq!(report["endpoint"]["label"], "POST /api/orders");
    assert!(
        report["truncated"].as_bool().unwrap_or(false)
            || report["has_more"].as_bool().unwrap_or(false)
            || report["omitted_count"].as_u64().unwrap_or(0) > 0
            || report["completeness"]["complete"] == false,
        "bounded answers must expose truncation honestly: {report}"
    );
    assert!(
        !labels(&report).contains(&"GET /api/health"),
        "health must not displace the handler under the cap: {report}"
    );
    assert!(
        report["nodes"]
            .as_array()
            .into_iter()
            .flatten()
            .any(|node| {
                node["kind"].as_str() != Some("endpoint")
                    && (node["label"].as_str().is_some_and(|label| {
                        !label.starts_with("GET ") && !label.starts_with("POST ")
                    }) || node["id"].as_str().is_some_and(|id| {
                        id.contains("orders.js")
                            || id.contains("middleware.js")
                            || id.contains("server.js")
                    }))
            }),
        "handler-side node must survive max_nodes=3: {report}"
    );
}
