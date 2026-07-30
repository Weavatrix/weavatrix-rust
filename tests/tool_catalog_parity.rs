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
    use std::collections::BTreeSet;
    use weavatrix_rust::tools;

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
    use blazingly_json::json;
    use weavatrix_rust::tools;

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
    #[cfg(feature = "clone")]
    {
        let duplicates = catalog
            .iter()
            .find(|tool| tool.name == "find_duplicates")
            .unwrap();
        let threshold = &duplicates.input_schema["properties"]["min_similarity"];
        assert_eq!(threshold["type"], "number");
        assert_eq!(threshold["minimum"], 0);
        assert_eq!(threshold["maximum"], 100);
        assert_eq!(
            threshold["description"],
            "0..1 is a fraction; values above 1 through 100 are percentages"
        );
    }
    let memory = catalog
        .iter()
        .find(|tool| tool.name == "memory_context")
        .unwrap();
    assert_eq!(
        memory.input_schema["required"],
        json!(["events", "request"])
    );

    let code = tools::catalog_for_profile(tools::ToolProfile::Code);
    assert!(code.iter().all(|tool| tool.name != "seo_link_suggestions"));
    let seo = tools::catalog_for_profile(tools::ToolProfile::Seo);
    assert!(seo.iter().any(|tool| tool.name == "seo_link_suggestions"));
    assert!(seo.iter().all(|tool| tool.name != "verified_change"));
}
