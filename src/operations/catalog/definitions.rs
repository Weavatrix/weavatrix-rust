pub(super) struct ToolSpec {
    pub name: &'static str,
    pub description: &'static str,
    pub required: &'static [&'static str],
}

pub(super) const SPECS: &[ToolSpec] = &[
    ToolSpec {
        name: "graph_stats",
        description: super::prose::graph::GRAPH_STATS,
        required: &[],
    },
    ToolSpec {
        name: "get_node",
        description: super::prose::graph::GET_NODE,
        required: &["label"],
    },
    ToolSpec {
        name: "get_neighbors",
        description: super::prose::graph::GET_NEIGHBORS,
        required: &["label"],
    },
    ToolSpec {
        name: "query_graph",
        description: super::prose::graph::QUERY_GRAPH,
        required: &[],
    },
    ToolSpec {
        name: "god_nodes",
        description: super::prose::graph::GOD_NODES,
        required: &[],
    },
    ToolSpec {
        name: "shortest_path",
        description: super::prose::graph::SHORTEST_PATH,
        required: &["source", "target"],
    },
    ToolSpec {
        name: "get_dependents",
        description: super::prose::change::GET_DEPENDENTS,
        required: &[],
    },
    ToolSpec {
        name: "change_impact",
        description: super::prose::change::CHANGE_IMPACT,
        required: &[],
    },
    ToolSpec {
        name: "map_stacktrace",
        description: super::prose::source::MAP_STACKTRACE,
        required: &["text"],
    },
    ToolSpec {
        name: "select_tests",
        description: super::prose::change::SELECT_TESTS,
        required: &[],
    },
    ToolSpec {
        name: "git_history",
        description: super::prose::change::GIT_HISTORY,
        required: &[],
    },
    ToolSpec {
        name: "git_read_blob",
        description: super::prose::change::GIT_READ_BLOB,
        required: &[],
    },
    ToolSpec {
        name: "cross_repo_git",
        description: super::prose::change::CROSS_REPO_GIT,
        required: &["repositories"],
    },
    ToolSpec {
        name: "git_fetch",
        description: super::prose::change::GIT_FETCH,
        required: &["url"],
    },
    ToolSpec {
        name: "verified_change",
        description: super::prose::change::VERIFIED_CHANGE,
        required: &["task"],
    },
    ToolSpec {
        name: "trace_api_contract",
        description: super::prose::source::TRACE_API_CONTRACT,
        required: &["backend", "clients"],
    },
    ToolSpec {
        name: "get_community",
        description: super::prose::graph::GET_COMMUNITY,
        required: &["community_id"],
    },
    ToolSpec {
        name: "search_code",
        description: super::prose::source::SEARCH_CODE,
        required: &["query"],
    },
    ToolSpec {
        name: "read_source",
        description: super::prose::source::READ_SOURCE,
        required: &[],
    },
    ToolSpec {
        name: "inspect_symbol",
        description: super::prose::source::INSPECT_SYMBOL,
        required: &[],
    },
    ToolSpec {
        name: "go_to_definition",
        description: super::prose::source::GO_TO_DEFINITION,
        required: &[],
    },
    ToolSpec {
        name: "find_references",
        description: super::prose::source::FIND_REFERENCES,
        required: &[],
    },
    ToolSpec {
        name: "context_bundle",
        description: super::prose::source::CONTEXT_BUNDLE,
        required: &[],
    },
    ToolSpec {
        name: "find_duplicates",
        description: super::prose::health::FIND_DUPLICATES,
        required: &[],
    },
    ToolSpec {
        name: "find_dead_code",
        description: super::prose::health::FIND_DEAD_CODE,
        required: &[],
    },
    ToolSpec {
        name: "run_audit",
        description: super::prose::health::RUN_AUDIT,
        required: &[],
    },
    ToolSpec {
        name: "coverage_map",
        description: super::prose::health::COVERAGE_MAP,
        required: &[],
    },
    ToolSpec {
        name: "hot_path_review",
        description: super::prose::health::HOT_PATH_REVIEW,
        required: &[],
    },
    ToolSpec {
        name: "perf_attribution",
        description: super::prose::health::PERF_ATTRIBUTION,
        required: &["measurements_file", "metric"],
    },
    ToolSpec {
        name: "list_communities",
        description: super::prose::graph::LIST_COMMUNITIES,
        required: &[],
    },
    ToolSpec {
        name: "module_map",
        description: super::prose::graph::MODULE_MAP,
        required: &[],
    },
    ToolSpec {
        name: "build_graph",
        description: super::prose::graph::BUILD_GRAPH,
        required: &[],
    },
    ToolSpec {
        name: "list_endpoints",
        description: super::prose::source::LIST_ENDPOINTS,
        required: &[],
    },
    ToolSpec {
        name: "trace_endpoint",
        description: super::prose::source::TRACE_ENDPOINT,
        required: &["path"],
    },
    ToolSpec {
        name: "rebuild_graph",
        description: super::prose::health::REBUILD_GRAPH,
        required: &[],
    },
    ToolSpec {
        name: "graph_diff",
        description: super::prose::change::GRAPH_DIFF,
        required: &["base_ref"],
    },
    ToolSpec {
        name: "get_architecture_contract",
        description: super::prose::architecture::GET_ARCHITECTURE_CONTRACT,
        required: &[],
    },
    ToolSpec {
        name: "architecture_inventory",
        description: super::prose::architecture::ARCHITECTURE_INVENTORY,
        required: &[],
    },
    ToolSpec {
        name: "ci_restrictions",
        description: super::prose::architecture::CI_RESTRICTIONS,
        required: &[],
    },
    ToolSpec {
        name: "ci_protection",
        description: super::prose::architecture::CI_PROTECTION,
        required: &["files"],
    },
    ToolSpec {
        name: "explain_restriction",
        description: super::prose::architecture::EXPLAIN_RESTRICTION,
        required: &["id"],
    },
    ToolSpec {
        name: "prepare_change",
        description: super::prose::change::PREPARE_CHANGE,
        required: &["files"],
    },
    ToolSpec {
        name: "verify_architecture",
        description: super::prose::architecture::VERIFY_ARCHITECTURE,
        required: &[],
    },
    ToolSpec {
        name: "verify_capabilities",
        description: super::prose::architecture::VERIFY_CAPABILITIES,
        required: &[],
    },
    ToolSpec {
        name: "explain_architecture_violation",
        description: super::prose::architecture::EXPLAIN_ARCHITECTURE_VIOLATION,
        required: &["fingerprint"],
    },
    ToolSpec {
        name: "propose_architecture_exception",
        description: super::prose::architecture::PROPOSE_ARCHITECTURE_EXCEPTION,
        required: &["fingerprint", "reason"],
    },
    ToolSpec {
        name: "open_repo",
        description: super::prose::health::OPEN_REPO,
        required: &["path"],
    },
    ToolSpec {
        name: "list_known_repos",
        description: super::prose::health::LIST_KNOWN_REPOS,
        required: &[],
    },
    ToolSpec {
        name: "semantic_link",
        description: super::prose::health::SEMANTIC_LINK,
        required: &[],
    },
];
