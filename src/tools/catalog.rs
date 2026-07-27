use serde::Serialize;
use serde_json::{Map, Value, json};

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolDefinition {
    pub name: &'static str,
    pub description: &'static str,
    pub input_schema: Value,
}

#[must_use]
#[allow(clippy::too_many_lines)]
pub fn catalog() -> Vec<ToolDefinition> {
    let mut tools = vec![
        tool(
            "graph_stats",
            "Graph size, evidence and build freshness.",
            &[],
        ),
        tool("get_node", "Resolve one exact graph node.", &["label"]),
        tool(
            "get_neighbors",
            "Direct typed incoming and outgoing relationships.",
            &["label"],
        ),
        tool(
            "query_graph",
            "Bounded BFS or DFS around exact or textual seeds.",
            &[],
        ),
        tool("god_nodes", "Rank high-connectivity production nodes.", &[]),
        tool(
            "shortest_path",
            "Shortest typed dependency path between two nodes.",
            &["source", "target"],
        ),
        tool(
            "get_dependents",
            "Bounded transitive reverse blast radius.",
            &["label"],
        ),
        tool(
            "change_impact",
            "Read-only Git change impact with graph evidence.",
            &[],
        ),
        tool(
            "git_history",
            "Bounded direct Git history without launching git.",
            &[],
        ),
        tool(
            "cross_repo_git",
            "Parallel histories, shared commits, or diffs across local repositories.",
            &["repositories"],
        ),
        tool(
            "verified_change",
            "Composite pre-commit evidence and conservative verdict.",
            &["task"],
        ),
        tool(
            "trace_api_contract",
            "Cross-repository endpoint contract evidence.",
            &["backend", "clients"],
        ),
        tool(
            "get_community",
            "Return one weak graph component.",
            &["community_id"],
        ),
        tool(
            "search_code",
            "Literal or Rust-regex repository search without ripgrep.",
            &["query"],
        ),
        tool(
            "read_source",
            "Bounded source context by node or repository path.",
            &[],
        ),
        tool(
            "inspect_symbol",
            "Definition, direct relationships and source evidence.",
            &["label"],
        ),
        tool(
            "context_bundle",
            "Compact graph and source bundle for one symbol.",
            &["label"],
        ),
        tool(
            "find_duplicates",
            "Deterministic Type-1/2/3 clone families.",
            &[],
        ),
        tool(
            "find_dead_code",
            "Conservative unreferenced-symbol review queue.",
            &[],
        ),
        tool(
            "run_audit",
            "Repository structure and evidence completeness audit.",
            &[],
        ),
        tool(
            "coverage_map",
            "Measured coverage discovery or explicit static reachability.",
            &[],
        ),
        tool(
            "hot_path_review",
            "Rank high-connectivity and large source symbols.",
            &[],
        ),
        tool(
            "list_communities",
            "List deterministic weak graph components.",
            &[],
        ),
        tool("module_map", "Production folder and dependency map.", &[]),
        tool(
            "list_endpoints",
            "Inventory statically extracted HTTP endpoints.",
            &[],
        ),
        tool(
            "trace_endpoint",
            "Resolve an endpoint and its bounded call neighborhood.",
            &["path"],
        ),
        tool(
            "rebuild_graph",
            "Rebuild the derived in-memory graph without source writes.",
            &[],
        ),
        tool(
            "graph_diff",
            "Compare the current snapshot with an immutable Git revision.",
            &["base_ref"],
        ),
        tool(
            "get_architecture_contract",
            "Read or preview the local target-architecture contract.",
            &[],
        ),
        tool(
            "prepare_change",
            "Select architecture rules for intended changed files.",
            &["files"],
        ),
        tool(
            "verify_architecture",
            "Verify graph dependencies against the active contract.",
            &[],
        ),
        tool(
            "explain_architecture_violation",
            "Explain one active contract violation.",
            &["fingerprint"],
        ),
        tool(
            "propose_architecture_exception",
            "Return a reviewable exception proposal without writing it.",
            &["fingerprint", "reason"],
        ),
        tool(
            "open_repo",
            "Retarget to another local repository.",
            &["path"],
        ),
        tool(
            "list_known_repos",
            "List repositories opened by this server process.",
            &[],
        ),
        tool(
            "semantic_link",
            "Build inferred semantic graph evidence from supplied vectors.",
            &["vectors"],
        ),
        tool(
            "vector_search",
            "Exact or bounded approximate nearest-neighbor search.",
            &["vectors", "query"],
        ),
        tool(
            "seo_link_suggestions",
            "Directional SEO internal-link evidence from supplied page profiles.",
            &["vectors", "pages"],
        ),
        tool(
            "memory_context",
            "Compile bounded temporal memory context from supplied events.",
            &["events", "request"],
        ),
    ];
    tools.retain(|tool| capability_is_compiled(tool.name));
    tools
}

#[must_use]
pub fn catalog_for_profile(profile: crate::mcp::McpProfile) -> Vec<ToolDefinition> {
    catalog()
        .into_iter()
        .filter(|tool| profile.allows(tool.name))
        .collect()
}

#[allow(clippy::needless_bool)]
fn capability_is_compiled(tool: &str) -> bool {
    if tool == "find_duplicates" {
        cfg!(feature = "clone")
    } else if matches!(
        tool,
        "change_impact" | "git_history" | "cross_repo_git" | "verified_change" | "graph_diff"
    ) {
        cfg!(feature = "git")
    } else if tool == "search_code" {
        cfg!(feature = "search")
    } else if matches!(tool, "semantic_link" | "seo_link_suggestions") {
        cfg!(feature = "semantic")
    } else if tool == "vector_search" {
        cfg!(feature = "vector")
    } else if tool == "memory_context" {
        cfg!(feature = "memory")
    } else {
        true
    }
}

fn tool(
    tool_name: &'static str,
    description: &'static str,
    required: &[&'static str],
) -> ToolDefinition {
    let mut properties = Map::from_iter([(
        "output_format".to_owned(),
        json!({
            "type": "string",
            "enum": ["text", "json"],
            "default": "json"
        }),
    )]);
    for name in super::catalog_schema::optional_fields(tool_name) {
        properties.insert(
            (*name).to_owned(),
            super::catalog_schema::field_schema(tool_name, name),
        );
    }
    for name in required {
        let schema = super::catalog_schema::field_schema(tool_name, name);
        properties.insert((*name).to_owned(), schema);
    }
    let required = required
        .iter()
        .map(|value| json!(value))
        .collect::<Vec<_>>();
    ToolDefinition {
        name: tool_name,
        description,
        input_schema: json!({
            "type": "object",
            "additionalProperties": true,
            "properties": properties,
            "required": required
        }),
    }
}
