pub(super) struct ToolSpec {
    pub name: &'static str,
    pub description: &'static str,
    pub required: &'static [&'static str],
}

pub(super) const SPECS: &[ToolSpec] = &[
    ToolSpec {
        name: "graph_stats",
        description: "Graph size, evidence and build freshness. evidence counts edges, not a percentage; evidence_share is that split. freshness.state CURRENT means the loaded snapshot, not a new disk walk.",
        required: &[],
    },
    ToolSpec {
        name: "get_node",
        description: "Resolve one exact graph node.",
        required: &["label"],
    },
    ToolSpec {
        name: "get_neighbors",
        description: "Direct typed incoming and outgoing relationships. The seed and each neighbor node are id, label, kind, language, and span unless response_detail is full. A compact row also carries direction, relation, and, when present, coupling and the call line. Test and classified neighbors stay out unless include_tests or include_classified is set.",
        required: &["label"],
    },
    ToolSpec {
        name: "query_graph",
        description: "Bounded BFS or DFS from exact seeds or a ranked question. Each node is id, label, kind, language, and span. Each edge is source, target, kind, and, when present, coupling and the call line. Call get_node for attributes.",
        required: &[],
    },
    ToolSpec {
        name: "god_nodes",
        description: "Rank high-connectivity production nodes. Skips files, modules, constants, stylesheets, and ubiquitous type names. Each node is id, label, kind, language, and span. Rank prefers callers in other files.",
        required: &[],
    },
    ToolSpec {
        name: "shortest_path",
        description: "Shortest typed path with per-hop witnesses, path kind, and work limits.",
        required: &["source", "target"],
    },
    ToolSpec {
        name: "get_dependents",
        description: "Bounded transitive reverse blast radius. Pass label for one symbol, or path for the declarations in that file. repository reads a child directory of the launch root and does not move the process pin. The seed and each dependent are id, label, kind, language, and span.",
        required: &[],
    },
    ToolSpec {
        name: "change_impact",
        description: "Read-only blast radius. A named file with no measured diff seeds its symbols as change file_scope (hypothetical, not a git edit). status stays COMPLETE when the tool ran. Read evidence_completeness and stop_reason. MAX_NODES means the cap omitted nodes. Impacted nodes are id, label, kind, language, and span. Does not read measured coverage.",
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
        description: "Bounded direct Git history without launching git. Pass path for the history of one file or directory; otherwise the result is repository HEAD history.",
        required: &[],
    },
    ToolSpec {
        name: "git_read_blob",
        description: "Bounded UTF-8 file content at an immutable Git revision or blob OID; binary blobs are refused.",
        required: &[],
    },
    ToolSpec {
        name: "cross_repo_git",
        description: "Parallel histories, shared commits, or diffs across named local repositories.",
        required: &["repositories"],
    },
    ToolSpec {
        name: "git_fetch",
        description: "Shallow-clone one Git URL at one ref into a local cache and return that checkout. Depth is 1. This launches git and writes only the cache. The process root stays pinned. git_history still reads the active repository.",
        required: &["url"],
    },
    ToolSpec {
        name: "verified_change",
        description: "Composite pre-commit evidence and conservative verdict. The blast radius is changeImpact. impact_depth and max_impact_nodes bound that radius. Selected symbols are compact nodes. Data-flow edges are source, target, kind, and, when present, coupling and the call line. Each edit context keeps at most 8 relationship rows.",
        required: &["task"],
    },
    ToolSpec {
        name: "trace_api_contract",
        description: "Cross-repository HTTP, GraphQL, gRPC and event-transport contract evidence for named backend and client roots.",
        required: &["backend", "clients"],
    },
    ToolSpec {
        name: "get_community",
        description: "Return one community: derived subsystems by default, or weak connectivity.",
        required: &["community_id"],
    },
    ToolSpec {
        name: "search_code",
        description: "Literal or Rust-regex search of the process-resident content index (backend liveindex). The first search in a process builds that index; later searches reuse it. A glob filters that index. The filesystem is scanned only when the widened index search is truncated and the filter kept fewer hits than requested. A glob does not force-include paths the snapshot skipped. This is not a graph-symbol search.",
        required: &["query"],
    },
    ToolSpec {
        name: "read_source",
        description: "Bounded source context by node or repository path.",
        required: &[],
    },
    ToolSpec {
        name: "inspect_symbol",
        description: "Definition, direct relationships and source evidence by label or source position.",
        required: &[],
    },
    ToolSpec {
        name: "go_to_definition",
        description: "Resolve a symbol to its definition. A source position does not guess by name. A label that matches one node resolves. A label that matches several returns candidates and does not pick. An unresolved position omits definition.",
        required: &[],
    },
    ToolSpec {
        name: "find_references",
        description: "Occurrences of the symbol at a position or label, from the graph and an on-disk SCIP index if present. An ambiguous label returns candidates instead of failing. Test and classified rows stay out unless include_tests or include_classified is set. The definition row stays.",
        required: &[],
    },
    ToolSpec {
        name: "context_bundle",
        description: "Task-aware source bundle with category quotas and dedup. The subject is id, label, kind, language, and span. Relationship rows match compact get_neighbors.",
        required: &[],
    },
    ToolSpec {
        name: "find_duplicates",
        description: "Deterministic Type-1/2/3 clone families. Files named .gen. or .generated., and files whose header says they were generated by openapi-generator.tech, are boilerplate unless include_boilerplate is set.",
        required: &[],
    },
    ToolSpec {
        name: "find_dead_code",
        description: "Conservative unreferenced-symbol review queue. Go main.go and func main are entry points. Exported symbols stay at confidence 50 and are omitted when min_confidence is higher. Each candidate is id, label, kind, language, and span. A type-only import does not keep its target live.",
        required: &[],
    },
    ToolSpec {
        name: "run_audit",
        description: "Repository structure and evidence completeness audit. Cycles are production file-level runtime dependencies, not containment or type-only imports. A named category collects only that section. At least 200 parsed edges with resolved edges under one fifth of that is REVIEW. Pass include_capabilities for the build capability matrix.",
        required: &[],
    },
    ToolSpec {
        name: "coverage_map",
        description: "Ingest LCOV, Istanbul, Tarpaulin, or LLVM reports onto graph nodes. Does not run tests. A missing report is unmeasured, not 0%. Quality writes `.weavatrix/coverage/lcov.info`.",
        required: &[],
    },
    ToolSpec {
        name: "hot_path_review",
        description: "Rank functions by static complexity times resolved call fan-in.",
        required: &[],
    },
    ToolSpec {
        name: "perf_attribution",
        description: "Correlate a measurement series with the declarations that changed between the revisions that produced it.",
        required: &["measurements_file", "metric"],
    },
    ToolSpec {
        name: "list_communities",
        description: "List derived subsystems by default, or weak coupling connectivity.",
        required: &[],
    },
    ToolSpec {
        name: "module_map",
        description: "Production folder map at a requested directory depth.",
        required: &[],
    },
    ToolSpec {
        name: "build_graph",
        description: "Workspace, target and runner topology from manifest evidence.",
        required: &[],
    },
    ToolSpec {
        name: "list_endpoints",
        description: "Inventory statically extracted HTTP endpoints as compact nodes (id, label, kind, language, span). max_results pages the list and reports endpoints_total; omit it to return every visible endpoint. repository reads a child directory of the launch root and does not move the process pin. Includes hand-rolled req.method/pathname conditions and Next.js App Router route files: an exported HTTP method under an app directory, with the URL taken from the file path. A Go struct that embeds Unimplemented*Server contributes GRPC Service/Method. A file whose first statement is \"use server\" contributes ACTION /path/name for each exported function. Page, layout, and route modules stay on their own rules.",
        required: &[],
    },
    ToolSpec {
        name: "trace_endpoint",
        description: "Resolve an HTTP endpoint by exact method/path and return its bounded execution/call path.",
        required: &["path"],
    },
    ToolSpec {
        name: "rebuild_graph",
        description: "Rebuild the derived in-memory graph without source writes. Returns node and edge counts before and after. Call graph_stats for the census.",
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
        name: "architecture_inventory",
        description: "Infer evidence-bounded architecture hypotheses (modular source, onion, ports-and-adapters, layered, microservices) from observed code and typed dependencies. Defaults to a bounded summary; detail=full returns paged graph evidence. Declared target style is not evidence.",
        required: &[],
    },
    ToolSpec {
        name: "ci_restrictions",
        description: "Local GitHub Actions jobs, literal check invocations, configured restrictions and unobserved enforcement.",
        required: &[],
    },
    ToolSpec {
        name: "ci_protection",
        description: "Candidate local checks and declared architecture-rule bindings for an explicit file list. Membership and component ids in the answer are path labels. Does not run workflows and does not read the worktree diff. change_impact, graph_diff, verified_change, and prepare_change do not include this block.",
        required: &["files"],
    },
    ToolSpec {
        name: "explain_restriction",
        description: "Explain one local restriction with its invocation, applicability and evidence limits.",
        required: &["id"],
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
        name: "verify_capabilities",
        description: "Resolve declared capabilities against exposed endpoint evidence.",
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
        description: "Retarget to another local repository only when this process was started with --allow-retarget. Otherwise the launch root stays fixed. Graphs unused for 20 minutes unload.",
        required: &["path"],
    },
    ToolSpec {
        name: "list_known_repos",
        description: "List in-process graphs and recent_requests: agent, repository, tool, time, reused, and ok. requests_total counts completed calls.",
        required: &[],
    },
    ToolSpec {
        name: "semantic_link",
        description: "Rank symbols for a text query with the deterministic local token hash weavatrix-token-hash-v1 (name, path, nearby doc comment, declaration line, and a short body; test paths rank lower; not a neural embedding), or link a non-empty caller-supplied vectors array. An empty vectors array without a query is an error.",
        required: &[],
    },
];
