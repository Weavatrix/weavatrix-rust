pub(super) struct ToolSpec {
    pub name: &'static str,
    pub description: &'static str,
    pub required: &'static [&'static str],
}

pub(super) const SPECS: &[ToolSpec] = &[
    ToolSpec {
        name: "graph_stats",
        description: "Graph size, evidence and build freshness.",
        required: &[],
    },
    ToolSpec {
        name: "get_node",
        description: "Resolve one exact graph node.",
        required: &["label"],
    },
    ToolSpec {
        name: "get_neighbors",
        description: "Direct typed incoming and outgoing relationships.",
        required: &["label"],
    },
    ToolSpec {
        name: "query_graph",
        description: "Bounded BFS or DFS around exact or textual seeds.",
        required: &[],
    },
    ToolSpec {
        name: "god_nodes",
        description: "Rank high-connectivity production nodes.",
        required: &[],
    },
    ToolSpec {
        name: "shortest_path",
        description: "Shortest typed dependency path between two nodes.",
        required: &["source", "target"],
    },
    ToolSpec {
        name: "get_dependents",
        description: "Bounded transitive reverse blast radius.",
        required: &["label"],
    },
    ToolSpec {
        name: "change_impact",
        description: "Read-only Git change impact with graph evidence.",
        required: &[],
    },
    ToolSpec {
        name: "map_stacktrace",
        description: "Map stack-trace text onto repository files and symbols.",
        required: &["text"],
    },
    ToolSpec {
        name: "select_tests",
        description: "Select the test suites a change most plausibly needs to run.",
        required: &[],
    },
    ToolSpec {
        name: "git_history",
        description: "Bounded direct Git history without launching git.",
        required: &[],
    },
    ToolSpec {
        name: "cross_repo_git",
        description: "Parallel histories, shared commits, or diffs across local repositories.",
        required: &["repositories"],
    },
    ToolSpec {
        name: "verified_change",
        description: "Composite pre-commit evidence and conservative verdict.",
        required: &["task"],
    },
    ToolSpec {
        name: "trace_api_contract",
        description: "Cross-repository HTTP, GraphQL, gRPC and event-transport contract evidence.",
        required: &["backend", "clients"],
    },
    ToolSpec {
        name: "get_community",
        description: "Return one weak graph component.",
        required: &["community_id"],
    },
    ToolSpec {
        name: "search_code",
        description: "Literal or Rust-regex repository search without ripgrep.",
        required: &["query"],
    },
    ToolSpec {
        name: "read_source",
        description: "Bounded source context by node or repository path.",
        required: &[],
    },
    ToolSpec {
        name: "inspect_symbol",
        description: "Definition, direct relationships and source evidence.",
        required: &["label"],
    },
    ToolSpec {
        name: "context_bundle",
        description: "Compact graph and source bundle for one symbol.",
        required: &["label"],
    },
    ToolSpec {
        name: "find_duplicates",
        description: "Deterministic Type-1/2/3 clone families.",
        required: &[],
    },
    ToolSpec {
        name: "find_dead_code",
        description: "Conservative unreferenced-symbol review queue.",
        required: &[],
    },
    ToolSpec {
        name: "run_audit",
        description: "Repository structure and evidence completeness audit.",
        required: &[],
    },
    ToolSpec {
        name: "coverage_map",
        description: "Measured coverage discovery or explicit static reachability.",
        required: &[],
    },
    ToolSpec {
        name: "hot_path_review",
        description: "Rank high-connectivity and large source symbols.",
        required: &[],
    },
    ToolSpec {
        name: "list_communities",
        description: "List deterministic weak graph components.",
        required: &[],
    },
    ToolSpec {
        name: "module_map",
        description: "Production folder and dependency map.",
        required: &[],
    },
    ToolSpec {
        name: "build_graph",
        description: "Workspace, target and runner topology from manifest evidence.",
        required: &[],
    },
    ToolSpec {
        name: "list_endpoints",
        description: "Inventory statically extracted HTTP endpoints.",
        required: &[],
    },
    ToolSpec {
        name: "trace_endpoint",
        description: "Resolve an endpoint and its bounded call neighborhood.",
        required: &["path"],
    },
    ToolSpec {
        name: "rebuild_graph",
        description: "Rebuild the derived in-memory graph without source writes.",
        required: &[],
    },
    ToolSpec {
        name: "graph_diff",
        description: "Compare the current snapshot with an immutable Git revision.",
        required: &["base_ref"],
    },
    ToolSpec {
        name: "get_architecture_contract",
        description: "Read or preview the local target-architecture contract.",
        required: &[],
    },
    ToolSpec {
        name: "prepare_change",
        description: "Select architecture rules for intended changed files.",
        required: &["files"],
    },
    ToolSpec {
        name: "verify_architecture",
        description: "Verify graph dependencies against the active contract.",
        required: &[],
    },
    ToolSpec {
        name: "explain_architecture_violation",
        description: "Explain one active contract violation.",
        required: &["fingerprint"],
    },
    ToolSpec {
        name: "propose_architecture_exception",
        description: "Return a reviewable exception proposal without writing it.",
        required: &["fingerprint", "reason"],
    },
    ToolSpec {
        name: "open_repo",
        description: "Retarget to another local repository.",
        required: &["path"],
    },
    ToolSpec {
        name: "list_known_repos",
        description: "List repositories opened by this server process.",
        required: &[],
    },
    ToolSpec {
        name: "semantic_link",
        description: "Build inferred semantic graph evidence from supplied vectors.",
        required: &["vectors"],
    },
    ToolSpec {
        name: "vector_search",
        description: "Exact or bounded approximate nearest-neighbor search.",
        required: &["vectors", "query"],
    },
    ToolSpec {
        name: "seo_link_suggestions",
        description: "Directional SEO internal-link evidence from supplied page profiles.",
        required: &["vectors", "pages"],
    },
    ToolSpec {
        name: "memory_context",
        description: "Compile bounded temporal memory context from supplied events.",
        required: &["events", "request"],
    },
];
