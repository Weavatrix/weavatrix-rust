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

use serde_json::{Value, json};
use support::GitFixture;
use weavatrix_memory::{
    AgentId, ContextRequest, EntityId, EventId, EventStore, ExpectedVersion, InMemoryStore,
    MemoryEvent, MemoryNode, NewEvent, SessionId, StreamId, Timestamp,
};
use weavatrix_rust::{NodeKind, Weavatrix, tools};

#[test]
fn exercises_the_read_only_tool_contract_end_to_end() {
    let backend = repository();
    let client = GitFixture::new();
    client.write("src/client.ts", "fetch('/api/items');\n");
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

fn graph_calls(ids: &[String]) -> Vec<(&'static str, Value)> {
    vec![
        ("graph_stats", json!({})),
        ("get_node", json!({"label": ids[0]})),
        (
            "get_neighbors",
            json!({"label": ids[0], "relation_filter": "calls"}),
        ),
        (
            "query_graph",
            json!({"seed_symbols": [ids[0]], "depth": 4, "mode": "dfs",
                "flow_direction": "both", "relation_filter": ["calls", "contains"]}),
        ),
        ("god_nodes", json!({"top_n": 20})),
        (
            "shortest_path",
            json!({"source": ids[0], "target": ids[1], "max_hops": 8}),
        ),
        ("get_dependents", json!({"label": ids[1], "depth": 4})),
        ("list_communities", json!({"top_n": 10})),
        ("get_community", json!({"community_id": 0})),
        ("module_map", json!({"top_n": 10})),
        ("list_endpoints", json!({"method": "GET"})),
        (
            "trace_endpoint",
            json!({"path": "/api/items", "method": "GET", "max_depth": 4}),
        ),
    ]
}

fn health_source_calls(ids: &[String]) -> Vec<(&'static str, Value)> {
    vec![
        (
            "search_code",
            json!({"query": "helper", "is_regex": false, "glob": "*.js",
                "before": 1, "after": 1, "max_results": 20}),
        ),
        (
            "read_source",
            json!({"label": ids[0], "before": 2, "after": 2}),
        ),
        ("inspect_symbol", json!({"label": ids[0]})),
        ("context_bundle", json!({"label": ids[0], "depth": 3})),
        (
            "find_duplicates",
            json!({"mode": "near_miss", "min_tokens": 12, "min_similarity": 70}),
        ),
        ("find_dead_code", json!({"top_n": 20})),
        ("run_audit", json!({"max_findings": 50})),
        ("coverage_map", json!({})),
        ("hot_path_review", json!({"top_n": 20})),
        ("get_architecture_contract", json!({})),
        (
            "prepare_change",
            json!({"files": ["app/main.js"], "intent": "test"}),
        ),
        ("verify_architecture", json!({})),
        ("list_known_repos", json!({})),
    ]
}

fn git_calls(engine: &mut Weavatrix, backend: &GitFixture, client: &GitFixture) {
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
            "trace_api_contract",
            json!({"backend": backend.root, "clients": [client.root]}),
        ),
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
    assert_eq!(opened["repository"], client.root.to_string_lossy().as_ref());
    tools::call(
        engine,
        "open_repo",
        json!({"path": backend.root.to_string_lossy()}),
    )
    .unwrap();
}

fn semantic_calls(engine: &mut Weavatrix, ids: &[String]) {
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
    assert!(
        tools::call(
            engine,
            "vector_search",
            json!({"vectors": vectors, "query": [1.0, 0.0, 0.0], "top_k": 2, "exact": true})
        )
        .is_ok()
    );
    let pages = json!([
        {"node": ids[0], "site": "docs", "canonical": "/alpha", "language": "en"},
        {"node": ids[1], "site": "docs", "canonical": "/beta", "language": "en",
            "cornerstone": true, "target_priority": 10}
    ]);
    assert!(
        tools::call(
            engine,
            "seo_link_suggestions",
            json!({"vectors": vectors, "pages": pages, "min_similarity": 0.5})
        )
        .is_ok()
    );
}

fn memory_call(engine: &mut Weavatrix) {
    let at = |value| Timestamp::from_unix_micros(value);
    let agent = AgentId::new("agent:test").unwrap();
    let session = SessionId::new("session:test").unwrap();
    let entity = EntityId::new("task:test").unwrap();
    let payload = MemoryEvent::NodeUpserted {
        node: MemoryNode::new(entity.clone(), "task", "Test task").unwrap(),
    };
    let event = NewEvent::new(
        EventId::new("event:test").unwrap(),
        payload.event_type(),
        at(1),
        at(1),
        agent,
        session,
        payload,
    )
    .unwrap();
    let mut store = InMemoryStore::default();
    store
        .append(
            &StreamId::new("stream:test").unwrap(),
            ExpectedVersion::NoStream,
            &[event],
        )
        .unwrap();
    let request = ContextRequest::new(vec![entity], at(2), at(2), 1_000).unwrap();
    let value = json!({"events": store.load_all(None, 100), "request": request});
    assert!(tools::call(engine, "memory_context", value).is_ok());
}

fn coverage_formats(engine: &mut Weavatrix, fixture: &GitFixture) {
    fixture.write(
        "lcov.info",
        "SF:app/main.js\nDA:1,1\nDA:2,0\nend_of_record\n",
    );
    let report = tools::call(engine, "coverage_map", json!({})).unwrap();
    assert_eq!(report["actualCoverage"], "AVAILABLE");
}

/// A contract written for the JavaScript engine names coupling kinds rather
/// than relation names. Matching nothing would report a passing verification,
/// so the vocabulary must either be evaluated or rejected out loud.
#[test]
fn coupling_kinds_are_evaluated_and_unknown_kinds_are_rejected() {
    let fixture = GitFixture::new();
    fixture.write("lib/util.ts", "export type Helper = { id: string };\n");
    fixture.write(
        "app/main.ts",
        "import type { Helper } from '../lib/util.ts';\nexport const use = (value: Helper) => value.id;\n",
    );
    fixture.write(
        "app/runtime.ts",
        "import { helper } from '../lib/impl.ts';\nexport const run = () => helper();\n",
    );
    fixture.write("lib/impl.ts", "export function helper(){ return 1; }\n");
    let contract = |kinds: &str| {
        format!(
            r#"{{"components":[{{"id":"app","paths":["app"]}},{{"id":"lib","paths":["lib"]}}],"dependencyRules":[{{"id":"no-app-lib","action":"forbid","from":["app"],"to":["lib"],"kinds":[{kinds}]}}],"ratchet":{{"baseline":{{"fingerprints":[]}}}}}}"#
        )
    };

    fixture.write(".weavatrix/architecture.json", &contract("\"runtime\""));
    let mut engine = Weavatrix::open(&fixture.root).unwrap();
    let runtime_only = tools::call(&mut engine, "verify_architecture", json!({})).unwrap();
    let flagged = runtime_only["new"]
        .as_array()
        .map(|items| {
            items
                .iter()
                .filter_map(|item| item["source"]["label"].as_str().map(str::to_owned))
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    assert!(
        flagged.iter().any(|label| label.contains("runtime.ts")),
        "a runtime import must violate a runtime rule, got {flagged:?}"
    );
    assert!(
        !flagged.iter().any(|label| label.contains("main.ts")),
        "an import type edge must not violate a runtime rule, got {flagged:?}"
    );

    fixture.write(".weavatrix/architecture.json", &contract("\"type-only\""));
    let mut engine = Weavatrix::open(&fixture.root).unwrap();
    let type_only = tools::call(&mut engine, "verify_architecture", json!({})).unwrap();
    let flagged = type_only["new"]
        .as_array()
        .map(|items| {
            items
                .iter()
                .filter_map(|item| item["source"]["label"].as_str().map(str::to_owned))
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    assert!(
        flagged.iter().any(|label| label.contains("main.ts")),
        "a type-only rule must catch the import type edge, got {flagged:?}"
    );

    fixture.write(
        ".weavatrix/architecture.json",
        &contract("\"compile-only\""),
    );
    let mut engine = Weavatrix::open(&fixture.root).unwrap();
    let error = tools::call(&mut engine, "verify_architecture", json!({}))
        .expect_err("an unevaluable kind must fail instead of passing");
    assert!(
        error.contains("compile-only"),
        "the rejection must name the unsupported kind, got {error}"
    );
}

fn repository() -> GitFixture {
    let fixture = GitFixture::new();
    fixture.write("lib/util.js", "export function helper(){ return 1; }\n");
    fixture.write(
        "app/main.js",
        "import { helper } from '../lib/util.js';\nexport function list(){ return helper(); }\nrouter.get('/api/items', list);\n",
    );
    let clone = "export function duplicate(value){ const a=value+1; const b=a*2; const c=b-3; return c+a+b+value; }\n";
    fixture.write("app/clone-a.js", clone);
    fixture.write("app/clone-b.js", clone);
    fixture.write(
        "package.json",
        r#"{"dependencies":{"unused":"1.0.0","express":"1.0.0"}}"#,
    );
    fixture.write(
        ".weavatrix/architecture.json",
        r#"{"components":[{"id":"app","paths":["app"]},{"id":"lib","paths":["lib"]}],"dependencyRules":[{"id":"no-app-lib","action":"forbid","from":["app"],"to":["lib"],"kinds":["imports"]}],"ratchet":{"baseline":{"fingerprints":[]}}}"#,
    );
    fixture.commit("baseline");
    fixture.write(
        "app/main.js",
        "import { helper } from '../lib/util.js';\nexport function list(){ return helper()+1; }\nrouter.get('/api/items', list);\n",
    );
    fixture.commit("change");
    fixture
}
