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

use blazingly_json::json;
use support::GitFixture;
use weavatrix_rust::{Weavatrix, tools};

#[test]
fn a_root_http_route_is_an_unmatched_contract_not_an_empty_search_error() {
    let backend = GitFixture::new();
    backend.write(
        "src/server.js",
        "function home() { return 'ok'; }\nrouter.get('/', home);\n",
    );
    backend.commit("root route");
    let client = GitFixture::new();
    client.write("src/client.js", "fetch('/api/items');\n");
    client.commit("unrelated client route");
    let mut engine = Weavatrix::open(&backend.root).unwrap();

    let result = tools::call(
        &mut engine,
        "trace_api_contract",
        json!({
            "backend": backend.root,
            "clients": [client.root],
            "transport": "http"
        }),
    )
    .expect("a root route must not issue an empty source query");

    assert_eq!(result["status"], "COMPLETE");
    assert_eq!(result["verdict"]["code"], "NO_STATIC_CLIENT_MATCH");
    assert_eq!(result["http"]["totals"]["endpoints"], 1);
    assert_eq!(result["http"]["totals"]["matches"], 0);
    assert_eq!(result["http"]["totals"]["unmatched_endpoints"], 1);
}
