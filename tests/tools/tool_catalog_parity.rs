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
        "map_stacktrace",
        "select_tests",
        "build_graph",
        "git_history",
        "git_read_blob",
        "cross_repo_git",
        "git_fetch",
        "verified_change",
        "trace_api_contract",
        "get_community",
        "search_code",
        "read_source",
        "inspect_symbol",
        "go_to_definition",
        "find_references",
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
        "architecture_inventory",
        "ci_restrictions",
        "ci_protection",
        "explain_restriction",
        "prepare_change",
        "verify_architecture",
        "verify_capabilities",
        "explain_architecture_violation",
        "propose_architecture_exception",
        "open_repo",
        "list_known_repos",
        "semantic_link",
        "n8n_inventory",
        "n8n_trace",
        "n8n_context",
        "dify_inventory",
        "dify_trace",
        "dify_context",
        "agent_inventory",
        "agent_trace",
        "agent_context",
        "agent_change_impact",
        "diagram_inventory",
        "diagram_trace",
        "diagram_context",
        "web3_inventory",
        "web3_trace",
        "web3_impact",
        "web3_context",
    ] {
        assert!(actual.contains(expected), "missing tool {expected}");
    }
    for closed in ["vector_search", "seo_link_suggestions", "memory_context"] {
        assert!(
            !actual.contains(closed),
            "{closed} is a library method, not an MCP tool"
        );
    }
    assert_eq!(actual.len(), 66, "the full catalog is 66 MCP tools");
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
    let architecture = catalog
        .iter()
        .find(|tool| tool.name == "architecture_inventory")
        .unwrap();
    assert_eq!(
        architecture.input_schema["properties"]["detail"]["default"],
        "summary"
    );
    assert_eq!(
        architecture.input_schema["properties"]["edge_cursor"]["type"],
        "string"
    );
    assert_eq!(
        architecture.input_schema["properties"]["max_results"]["type"],
        "integer"
    );
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
        assert_eq!(
            duplicates.input_schema["properties"]["include_declarative"]["default"], true,
            "duplicate review must advertise its high-recall default"
        );
    }
    let semantic = catalog
        .iter()
        .find(|tool| tool.name == "semantic_link")
        .unwrap();
    let vectors = &semantic.input_schema["properties"]["vectors"];
    assert_eq!(vectors["items"]["required"], json!(["node", "values"]));
    assert!(
        vectors["description"]
            .as_str()
            .unwrap()
            .contains("An empty array without query is an error"),
        "{vectors}"
    );
    for closed in ["vector_search", "seo_link_suggestions", "memory_context"] {
        assert!(catalog.iter().all(|tool| tool.name != closed));
    }

    assert_domain_profiles();

    #[cfg(feature = "git")]
    {
        let change_impact = catalog
            .iter()
            .find(|tool| tool.name == "change_impact")
            .unwrap();
        assert_eq!(
            change_impact.input_schema["properties"]["target"]["type"],
            "string"
        );
    }
}

#[test]
fn catalog_does_not_advertise_unavailable_lsp_precision() {
    use blazingly_json::json;
    use weavatrix_rust::tools;

    let catalog = tools::catalog();
    for name in ["get_dependents", "change_impact", "select_tests"] {
        // Feature-minimal builds compile only part of the catalog; a tool
        // that is absent cannot advertise a precision at all.
        let Some(definition) = catalog.iter().find(|tool| tool.name == name) else {
            continue;
        };
        assert_eq!(
            definition.input_schema["properties"]["precision"]["enum"],
            json!(["graph"]),
            "{name} must advertise the only precision it can execute"
        );
    }
}

#[test]
#[cfg(feature = "git")]
fn graph_diff_catalog_documents_its_compact_default_and_raw_edge_opt_in() {
    use blazingly_json::json;
    use weavatrix_rust::tools;

    let graph_diff = tools::catalog()
        .into_iter()
        .find(|tool| tool.name == "graph_diff")
        .unwrap();
    let detail = &graph_diff.input_schema["properties"]["detail"];

    assert_eq!(detail["enum"], json!(["file_pairs", "edges"]));
    assert_eq!(detail["default"], "file_pairs");
}

#[cfg(all(feature = "memory", feature = "semantic"))]
fn assert_domain_profiles() {
    use weavatrix_rust::tools::{self, ToolProfile};
    let code = tools::catalog_for_profile(ToolProfile::Code);
    assert_eq!(code.len(), tools::catalog().len());
    for closed in ["vector_search", "seo_link_suggestions", "memory_context"] {
        assert!(code.iter().all(|tool| tool.name != closed));
    }
    let seo = tools::catalog_for_profile(ToolProfile::Seo);
    assert!(seo.iter().any(|tool| tool.name == "semantic_link"));
    assert!(seo.iter().all(|tool| {
        tool.name != "seo_link_suggestions"
            && tool.name != "vector_search"
            && tool.name != "memory_context"
    }));
    assert!(seo.iter().all(|tool| tool.name != "verified_change"));
    let n8n = tools::catalog_for_profile(ToolProfile::N8n);
    assert!(n8n.iter().any(|tool| tool.name == "n8n_inventory"));
    assert!(n8n.iter().any(|tool| tool.name == "n8n_trace"));
    assert!(n8n.iter().any(|tool| tool.name == "n8n_context"));
    assert!(n8n.iter().all(|tool| tool.name != "verified_change"));
    assert!(n8n.iter().all(|tool| tool.name != "seo_link_suggestions"));
    let dify = tools::catalog_for_profile(ToolProfile::Dify);
    assert!(dify.iter().any(|tool| tool.name == "dify_inventory"));
    assert!(dify.iter().any(|tool| tool.name == "dify_trace"));
    assert!(dify.iter().any(|tool| tool.name == "dify_context"));
    assert!(dify.iter().all(|tool| tool.name != "verified_change"));
    assert!(dify.iter().all(|tool| tool.name != "n8n_inventory"));
    let agent = tools::catalog_for_profile(ToolProfile::Agent);
    assert!(agent.iter().any(|tool| tool.name == "agent_inventory"));
    assert!(agent.iter().any(|tool| tool.name == "agent_trace"));
    assert!(agent.iter().any(|tool| tool.name == "agent_context"));
    assert!(agent.iter().any(|tool| tool.name == "agent_change_impact"));
    assert!(agent.iter().all(|tool| tool.name != "verified_change"));
    assert!(agent.iter().all(|tool| tool.name != "n8n_inventory"));
    let diagram = tools::catalog_for_profile(ToolProfile::Diagram);
    assert!(diagram.iter().any(|tool| tool.name == "diagram_inventory"));
    assert!(diagram.iter().any(|tool| tool.name == "diagram_trace"));
    assert!(diagram.iter().any(|tool| tool.name == "diagram_context"));
    assert!(diagram.iter().all(|tool| tool.name != "verified_change"));
    let web3 = tools::catalog_for_profile(ToolProfile::Web3);
    assert!(web3.iter().any(|tool| tool.name == "web3_inventory"));
    assert!(web3.iter().any(|tool| tool.name == "web3_trace"));
    assert!(web3.iter().any(|tool| tool.name == "web3_impact"));
    assert!(web3.iter().any(|tool| tool.name == "web3_context"));
    assert!(web3.iter().all(|tool| tool.name != "verified_change"));
}
