#![cfg(all(
    feature = "clone",
    feature = "git",
    feature = "lang-rust",
    feature = "memory",
    feature = "search",
    feature = "semantic",
    feature = "vector"
))]

mod read_only_contract;
mod support;

use blazingly_json::json;
use read_only_contract::{
    coverage_formats, git_calls, graph_calls, health_source_calls, memory_call, repository,
    semantic_calls,
};
use support::GitFixture;
use weavatrix_rust::{NodeKind, Weavatrix, tools};

#[test]
fn exercises_the_read_only_tool_contract_end_to_end() {
    let backend = repository();
    let client = GitFixture::new();
    client.write(
        "src/client.ts",
        "import * as natsLib from 'nats';\nconst nc = natsLib.connect();\nfetch('/api/items');\nnc.subscribe('jobs');\n",
    );
    client.commit("client");
    let mut engine = Weavatrix::open(&backend.root).unwrap();

    let nodes = engine.state().graph().nodes();
    let function_ids = nodes
        .iter()
        .filter(|node| node.kind == NodeKind::Function)
        .take(2)
        .map(|node| node.id.to_string())
        .collect::<Vec<_>>();
    let file_ids = nodes
        .iter()
        .filter(|node| node.kind == NodeKind::File)
        .take(2)
        .map(|node| node.id.to_string())
        .collect::<Vec<_>>();
    assert_eq!(function_ids.len(), 2);
    assert_eq!(file_ids.len(), 2);

    for (name, input) in graph_calls(&function_ids) {
        let result =
            tools::call(&mut engine, name, input).unwrap_or_else(|error| panic!("{name}: {error}"));
        assert!(result.is_object(), "{name}");
    }
    let neighbors = tools::call(
        &mut engine,
        "get_neighbors",
        json!({"label": function_ids[0], "max_results": 1}),
    )
    .unwrap();
    assert!(neighbors["page"]["returned"].as_u64().unwrap() <= 1);
    assert!(neighbors["page"]["total"].as_u64().unwrap() >= 1);
    let community = tools::call(
        &mut engine,
        "get_community",
        json!({"community_id": 0, "max_nodes": 1}),
    )
    .unwrap();
    assert_eq!(community["page"]["returned"], 1);
    assert!(community["page"]["total"].as_u64().unwrap() >= 1);
    for (name, input) in health_source_calls(&function_ids) {
        let result =
            tools::call(&mut engine, name, input).unwrap_or_else(|error| panic!("{name}: {error}"));
        assert!(result.is_object(), "{name}");
    }

    let verification = tools::call(&mut engine, "verify_architecture", json!({})).unwrap();
    let fingerprint = verification["new"][0]["fingerprint"]
        .as_str()
        .unwrap()
        .to_owned();
    for (name, input) in [
        (
            "explain_architecture_violation",
            json!({"fingerprint": fingerprint}),
        ),
        (
            "propose_architecture_exception",
            json!({"fingerprint": fingerprint, "reason": "migration", "expires": "2026-12-31"}),
        ),
    ] {
        assert!(tools::call(&mut engine, name, input).unwrap().is_object());
    }

    git_calls(&mut engine, &backend, &client);
    semantic_calls(&mut engine, &file_ids);
    memory_call(&mut engine);
    coverage_formats(&mut engine, &backend);
    assert!(tools::call(&mut engine, "rebuild_graph", json!({})).is_ok());
    assert!(tools::call(&mut engine, "unknown", json!({})).is_err());
}
