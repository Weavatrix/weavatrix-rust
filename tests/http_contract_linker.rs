#![cfg(all(feature = "git", feature = "search"))]

mod support;

use blazingly_json::json;
use support::GitFixture;
use weavatrix_rust::{Weavatrix, tools};

fn trace_http(backend: &GitFixture, client: &GitFixture) -> blazingly_json::Value {
    let mut engine = Weavatrix::open(&backend.root).unwrap();
    tools::call(
        &mut engine,
        "trace_api_contract",
        json!({
            "backend": backend.root,
            "clients": [client.root],
            "transport": "http"
        }),
    )
    .expect("http contract trace")
}

fn post_orders_backend() -> GitFixture {
    let backend = GitFixture::new();
    backend.write(
        "src/server.js",
        "function create() { return 'ok'; }\nrouter.post('/api/orders', create);\n",
    );
    backend.commit("post orders");
    backend
}

#[test]
fn whitespace_insensitive_fetch_method_is_post() {
    let backend = post_orders_backend();
    for (name, source) in [
        ("spaced.js", "fetch('/api/orders', { method: 'POST' });\n"),
        ("tight.js", "fetch('/api/orders', {method:'POST'});\n"),
        ("double.js", "fetch('/api/orders', {method: \"POST\"});\n"),
        (
            "multiline.js",
            "fetch('/api/orders', {\n  method:'POST'\n});\n",
        ),
    ] {
        let client = GitFixture::new();
        client.write(&format!("src/{name}"), source);
        client.commit(name);
        let result = trace_http(&backend, &client);
        assert_eq!(result["http"]["totals"]["matches"], 1, "{name}");
        assert_eq!(result["http"]["totals"]["method_mismatches"], 0, "{name}");
        let method = &result["http"]["contracts"][0]["callsites"][0]["method"];
        assert_eq!(method, "POST", "{name}");
    }
}

#[test]
fn comment_only_route_mention_is_not_a_proven_callsite() {
    let backend = post_orders_backend();
    let client = GitFixture::new();
    client.write("src/client.js", "// TODO: POST /api/orders\n");
    client.commit("comment only");
    let result = trace_http(&backend, &client);
    assert_eq!(result["http"]["totals"]["matches"], 0);
    assert_eq!(result["http"]["totals"]["unmatched_endpoints"], 1);
}

#[test]
fn dynamic_options_are_unresolved_not_proven_get_mismatch() {
    let backend = post_orders_backend();
    let client = GitFixture::new();
    client.write("src/client.js", "fetch('/api/orders', options);\n");
    client.commit("dynamic options");
    let result = trace_http(&backend, &client);
    assert_eq!(result["http"]["totals"]["matches"], 1);
    assert_eq!(result["http"]["totals"]["method_mismatches"], 0);
    assert_eq!(
        result["http"]["contracts"][0]["callsites"][0]["method"],
        "UNRESOLVED"
    );
}

#[test]
fn bare_fetch_defaults_to_get_and_mismatches_post() {
    let backend = post_orders_backend();
    let client = GitFixture::new();
    client.write("src/client.js", "fetch('/api/orders');\n");
    client.commit("default get");
    let result = trace_http(&backend, &client);
    assert_eq!(result["http"]["totals"]["matches"], 1);
    assert_eq!(result["http"]["totals"]["method_mismatches"], 1);
    assert_eq!(
        result["http"]["contracts"][0]["callsites"][0]["method"],
        "GET"
    );
}
