use blazingly_json::json;
#[cfg(all(
    feature = "clone",
    feature = "git",
    feature = "memory",
    feature = "search",
    feature = "semantic",
    feature = "vector"
))]
use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};
use weavatrix_rust::{Weavatrix, tools};

#[test]
#[cfg(all(
    feature = "clone",
    feature = "git",
    feature = "memory",
    feature = "search",
    feature = "semantic",
    feature = "vector"
))]
fn catalog_covers_the_javascript_read_only_core_and_rust_extensions() {
    let actual = tools::catalog()
        .into_iter()
        .map(|tool| tool.name)
        .collect::<BTreeSet<_>>();
    for expected in [
        "graph_stats",
        "get_node",
        "get_neighbors",
        "query_graph",
        "god_nodes",
        "shortest_path",
        "get_dependents",
        "change_impact",
        "git_history",
        "cross_repo_git",
        "verified_change",
        "trace_api_contract",
        "get_community",
        "search_code",
        "read_source",
        "inspect_symbol",
        "context_bundle",
        "find_duplicates",
        "find_dead_code",
        "run_audit",
        "coverage_map",
        "hot_path_review",
        "list_communities",
        "module_map",
        "list_endpoints",
        "trace_endpoint",
        "rebuild_graph",
        "graph_diff",
        "get_architecture_contract",
        "prepare_change",
        "verify_architecture",
        "explain_architecture_violation",
        "propose_architecture_exception",
        "open_repo",
        "list_known_repos",
        "semantic_link",
        "vector_search",
        "seo_link_suggestions",
        "memory_context",
    ] {
        assert!(actual.contains(expected), "missing tool {expected}");
    }
}

#[test]
#[cfg(all(feature = "memory", feature = "semantic"))]
fn catalog_exposes_real_argument_contracts_and_profiles() {
    let catalog = tools::catalog();
    let query = catalog
        .iter()
        .find(|tool| tool.name == "query_graph")
        .unwrap();
    assert_eq!(query.input_schema["properties"]["depth"]["type"], "integer");
    assert_eq!(
        query.input_schema["properties"]["seed_files"]["items"]["type"],
        "string"
    );
    let memory = catalog
        .iter()
        .find(|tool| tool.name == "memory_context")
        .unwrap();
    assert_eq!(
        memory.input_schema["required"],
        json!(["events", "request"])
    );

    let code = tools::catalog_for_profile(weavatrix_rust::mcp::McpProfile::Code);
    assert!(code.iter().all(|tool| tool.name != "seo_link_suggestions"));
    let seo = tools::catalog_for_profile(weavatrix_rust::mcp::McpProfile::Seo);
    assert!(seo.iter().any(|tool| tool.name == "seo_link_suggestions"));
    assert!(seo.iter().all(|tool| tool.name != "verified_change"));
}

#[test]
#[cfg(all(
    feature = "lang-rust",
    feature = "search",
    feature = "semantic",
    feature = "vector"
))]
fn graph_search_source_and_semantic_tools_work_together() {
    let fixture = Fixture::new();
    fixture.write(
        "src/lib.rs",
        "pub fn alpha() { beta(); }\npub fn beta() {}\n",
    );
    fixture.write(
        "web/router.js",
        "function list() {}\nrouter.get(\"/items\", list);\n",
    );
    let mut engine = Weavatrix::open(&fixture.root).unwrap();

    let stats = tools::call(&mut engine, "graph_stats", json!({})).unwrap();
    assert!(stats["nodes"].as_u64().unwrap() >= 6);
    let search = tools::call(
        &mut engine,
        "search_code",
        json!({"query": "beta", "max_results": 10}),
    )
    .unwrap();
    assert!(search["occurrences"].as_u64().unwrap() >= 2);
    let endpoints = tools::call(&mut engine, "list_endpoints", json!({})).unwrap();
    assert_eq!(endpoints["endpoints"][0]["label"], "GET /items");

    let nodes = engine
        .state()
        .graph()
        .nodes()
        .iter()
        .filter(|node| node.label == "alpha" || node.label == "beta")
        .map(|node| node.id.to_string())
        .collect::<Vec<_>>();
    let semantic = tools::call(
        &mut engine,
        "semantic_link",
        json!({
            "min_similarity": 0.5,
            "vectors": [
                {"node": nodes[0], "values": [1.0, 0.0]},
                {"node": nodes[1], "values": [0.9, 0.1]}
            ]
        }),
    )
    .unwrap();
    assert!(
        semantic["edges"]
            .as_array()
            .is_some_and(|edges| !edges.is_empty())
    );

    let vector = tools::call(
        &mut engine,
        "vector_search",
        json!({
            "vectors": [
                {"node": "alpha", "values": [1.0, 0.0]},
                {"node": "beta", "values": [0.0, 1.0]}
            ],
            "query": [0.9, 0.1],
            "top_k": 1,
            "exact": true
        }),
    )
    .unwrap();
    assert_eq!(vector["hits"][0]["node"], "alpha");
}

#[test]
fn rust_tarpaulin_coverage_is_measured_not_static_reachability() {
    let fixture = Fixture::new();
    fixture.write("src/lib.rs", "pub fn covered() {}\n");
    fixture.write(
        "tarpaulin-report.json",
        r#"{
          "files": [{
            "path": ["src", "lib.rs"],
            "content": "pub fn covered() {}",
            "traces": [],
            "covered": 1,
            "coverable": 1
          }],
          "coverage": 100.0,
          "covered": 1,
          "coverable": 1
        }"#,
    );
    let mut engine = Weavatrix::open(&fixture.root).unwrap();
    let coverage = tools::call(&mut engine, "coverage_map", json!({})).unwrap();
    assert_eq!(coverage["actualCoverage"], "AVAILABLE");
    assert_eq!(coverage["files"][0]["lines_hit"], 1);
    assert_eq!(coverage["files"][0]["lines_found"], 1);
}

#[test]
fn audit_compares_external_imports_with_supported_manifests() {
    let fixture = Fixture::new();
    fixture.write("package.json", r#"{"dependencies":{"lodash":"1.0.0"}}"#);
    fixture.write(
        "src/server.ts",
        "import express from \"express\";\nexport const app = express();\n",
    );
    let mut engine = Weavatrix::open(&fixture.root).unwrap();
    let audit = tools::call(&mut engine, "run_audit", json!({})).unwrap();
    let findings = audit["dependency_report"]["findings"].as_array().unwrap();
    assert!(
        findings
            .iter()
            .any(|finding| finding["id"] == "dependency.missing:typescript:express")
    );
    assert!(findings.iter().any(|finding| {
        finding["rule"] == "dependency.unused_declaration" && finding["package"] == "lodash"
    }));
}

#[test]
#[cfg(feature = "lang-rust")]
fn incremental_refresh_rebuilds_only_after_source_changes() {
    let fixture = Fixture::new();
    fixture.write("src/lib.rs", "pub fn first() {}\n");
    let mut engine = Weavatrix::open(&fixture.root).unwrap();
    assert!(!engine.refresh_if_stale().unwrap());
    let before = engine.state().graph().node_count();

    fixture.write("src/lib.rs", "pub fn first() {}\npub fn second() {}\n");
    assert!(engine.refresh_if_stale().unwrap());
    assert!(engine.state().graph().node_count() > before);
}

struct Fixture {
    root: PathBuf,
}

impl Fixture {
    fn new() -> Self {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root =
            std::env::temp_dir().join(format!("weavatrix-tools-{}-{nonce}", std::process::id()));
        fs::create_dir_all(&root).unwrap();
        Self { root }
    }

    fn write(&self, relative: &str, contents: &str) {
        let path = self.root.join(relative);
        fs::create_dir_all(path.parent().unwrap_or(Path::new("."))).unwrap();
        fs::write(path, contents).unwrap();
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}
