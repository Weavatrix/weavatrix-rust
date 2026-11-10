use crate::support::GitFixture;
use blazingly_json::json;
use weavatrix_rust::{Weavatrix, tools};

fn assert_closed_library_tool(engine: &mut Weavatrix, name: &str) {
    let error = tools::call(engine, name, json!({})).expect_err(name);
    assert!(
        error.contains("is not an MCP tool"),
        "{name} stays in its library crate: {error}"
    );
}

pub(crate) fn git_calls(engine: &mut Weavatrix, backend: &GitFixture, client: &GitFixture) {
    let http = tools::call(
        engine,
        "trace_api_contract",
        json!({
            "backend": backend.root,
            "clients": [client.root],
            "transport": "http"
        }),
    )
    .unwrap();
    assert_eq!(http["status"], "COMPLETE");
    assert_eq!(http["verdict"]["code"], "MATCHED");
    assert_eq!(http["http"]["totals"]["matches"], 1);

    let event = tools::call(
        engine,
        "trace_api_contract",
        json!({
            "backend": backend.root,
            "clients": [client.root],
            "transport": "event"
        }),
    )
    .unwrap();
    assert_eq!(event["status"], "COMPLETE");
    assert_eq!(event["transport_contracts"]["status"], "COMPLETE");
    assert_eq!(event["transport_contracts"]["totals"]["matches"], 1);
    assert_eq!(event["transport_contracts"]["totals"]["ambiguities"], 0);

    for (name, input) in [
        ("git_history", json!({"max_commits": 20, "months": 1200})),
        (
            "change_impact",
            json!({"base_ref": "HEAD~1", "head_ref": "HEAD", "depth": 3}),
        ),
        (
            "verified_change",
            json!({"task": "review", "base_ref": "HEAD~1", "head_ref": "HEAD",
                "duplicate_ratchet": true}),
        ),
        ("graph_diff", json!({"base_ref": "HEAD~1"})),
        (
            "cross_repo_git",
            json!({"repositories": [
                {"name": "backend", "path": backend.root},
                {"name": "copy", "path": backend.root}
            ], "action": "shared_commits", "max_commits": 20}),
        ),
    ] {
        assert!(
            tools::call(engine, name, input).unwrap().is_object(),
            "{name}"
        );
    }
    let opened = tools::call(
        engine,
        "open_repo",
        json!({"path": client.root.to_string_lossy()}),
    )
    .unwrap();
    let opened_root = std::fs::canonicalize(
        opened["repository"]
            .as_str()
            .expect("open_repo returns a repository path"),
    )
    .unwrap();
    assert_eq!(opened_root, std::fs::canonicalize(&client.root).unwrap());
    tools::call(
        engine,
        "open_repo",
        json!({"path": backend.root.to_string_lossy(), "build": false}),
    )
    .unwrap();
}

pub(crate) fn semantic_calls(engine: &mut Weavatrix, ids: &[String]) {
    let vectors = json!([
        {"node": ids[0], "values": [1.0, 0.0, 0.0]},
        {"node": ids[1], "values": [0.9, 0.1, 0.0]}
    ]);
    assert!(
        tools::call(
            engine,
            "semantic_link",
            json!({"vectors": vectors, "min_similarity": 0.5, "selection": "mutual"})
        )
        .is_ok()
    );
    assert_closed_library_tool(engine, "vector_search");
    assert_closed_library_tool(engine, "seo_link_suggestions");
}

pub(crate) fn memory_call(engine: &mut Weavatrix) {
    assert_closed_library_tool(engine, "memory_context");
}

pub(crate) fn coverage_formats(engine: &mut Weavatrix, fixture: &GitFixture) {
    fixture.write(
        "lcov.info",
        "SF:app/main.js\nDA:1,1\nDA:2,0\nend_of_record\n",
    );
    let report = tools::call(engine, "coverage_map", json!({})).unwrap();
    assert_eq!(report["status"], "COMPLETE");
    assert_eq!(report["measured_coverage"]["present"], true);
}
