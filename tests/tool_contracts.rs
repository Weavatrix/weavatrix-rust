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
use weavatrix_memory::{
    AgentId, ContextRequest, EntityId, EventId, EventStore, ExpectedVersion, InMemoryStore,
    MemoryEvent, MemoryNode, NewEvent, SessionId, StreamId, Timestamp,
};
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

#[test]
fn absent_optional_configuration_and_routes_are_structured_results() {
    let fixture = GitFixture::new();
    fixture.write(
        "src/main.js",
        "export function main() { return 'configured by code only'; }\n",
    );
    fixture.commit("without architecture contract");
    let mut engine = Weavatrix::open(&fixture.root).unwrap();

    for (tool, args) in [
        (
            "prepare_change",
            json!({"files": ["src/main.js"], "intent": "inspect"}),
        ),
        ("verify_architecture", json!({})),
        (
            "explain_architecture_violation",
            json!({"fingerprint": "missing"}),
        ),
        (
            "propose_architecture_exception",
            json!({"fingerprint": "missing", "reason": "none"}),
        ),
    ] {
        let result = tools::call(&mut engine, tool, args)
            .unwrap_or_else(|error| panic!("{tool} must not expose an IO error: {error}"));
        assert_eq!(
            result["state"], "NOT_CONFIGURED",
            "{tool} must make the optional configuration state explicit"
        );
    }

    let missing = tools::call(
        &mut engine,
        "trace_endpoint",
        json!({"path": "/absent", "method": "GET"}),
    )
    .expect("an absent endpoint is a query result, not a tool failure");
    assert_eq!(missing["state"], "NOT_FOUND");
    assert_eq!(missing["endpoint"], Value::Null);
    assert_eq!(missing["nodes"], json!([]));
}

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
    assert_eq!(report["status"], "COMPLETE");
    assert_eq!(report["measured_coverage"]["present"], true);
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
        "import { helper } from '../lib/util.js';\nimport * as natsLib from 'nats';\nconst nc = natsLib.connect();\nexport function list(){ return helper(); }\nrouter.get('/api/items', list);\nnc.publish('jobs', new Uint8Array());\n",
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
        "import { helper } from '../lib/util.js';\nimport * as natsLib from 'nats';\nconst nc = natsLib.connect();\nexport function list(){ return helper()+1; }\nrouter.get('/api/items', list);\nnc.publish('jobs', new Uint8Array());\n",
    );
    fixture.commit("change");
    fixture
}
