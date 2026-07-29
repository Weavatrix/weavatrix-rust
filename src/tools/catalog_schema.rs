use blazingly_json::{Value, json};

#[allow(clippy::too_many_lines)]
pub(super) fn optional_fields(tool: &str) -> &'static [&'static str] {
    match tool {
        "get_neighbors" => &[
            "relation_filter",
            "max_results",
            "cursor",
            "response_detail",
        ],
        "query_graph" => &[
            "question",
            "mode",
            "depth",
            "max_nodes",
            "context_filter",
            "seed_files",
            "seed_symbols",
            "relation_filter",
            "flow_direction",
            "augment_seeds",
            "include_classified",
            "include_low_signal",
            "token_budget",
        ],
        "god_nodes" => &["top_n", "include_classified"],
        "shortest_path" => &["max_hops"],
        "get_dependents" => &[
            "depth",
            "max_nodes",
            "precision",
            "max_references",
            "timeout_ms",
            "include_container_importers",
        ],
        "change_impact" => &[
            "base",
            "base_ref",
            "head_ref",
            "diff",
            "files",
            "depth",
            "max_nodes",
            "precision",
            "max_references",
            "timeout_ms",
        ],
        "git_history" => &[
            "revision",
            "months",
            "max_commits",
            "min_pair_count",
            "max_pairs",
            "top_n",
            "first_parent",
        ],
        "cross_repo_git" => &[
            "action",
            "revision",
            "base_ref",
            "head_ref",
            "max_commits",
            "first_parent",
            "left",
            "right",
        ],
        "verified_change" => &[
            "phase",
            "base_ref",
            "head_ref",
            "diff",
            "files",
            "precision",
            "max_symbols",
            "impact_depth",
            "max_impact_nodes",
            "data_flow_depth",
            "max_data_flow_edges",
            "duplicate_ratchet",
            "api_contract",
            "tests",
            "run_tests",
            "test_timeout_ms",
        ],
        "trace_api_contract" => &[
            "transport",
            "method",
            "path",
            "changed_files",
            "client_names",
            "include_tests",
            "include_classified",
            "max_impact_depth",
            "max_endpoints",
            "max_matches",
            "max_source_files",
            "max_source_file_bytes",
            "max_affected_files",
            "top_n",
            "client_wrappers",
            "auto_discover_wrappers",
            "runtime_config",
            "runtime_evidence_files",
            "runtime_evidence_max_age_hours",
            "response_detail",
            "page_size",
            "per_item_limit",
            "cursor",
        ],
        "search_code" => &["is_regex", "glob", "before", "after", "max_results"],
        "read_source" => &["label", "path", "start_line", "before", "after"],
        "inspect_symbol" => &[
            "precision",
            "max_references",
            "max_containers",
            "context_lines",
            "timeout_ms",
        ],
        "context_bundle" => &[
            "precision",
            "max_references",
            "max_related",
            "max_reexports",
            "max_source_files",
            "context_lines",
            "include_classified",
            "timeout_ms",
        ],
        "find_duplicates" => &[
            "min_similarity",
            "min_tokens",
            "mode",
            "include_tests",
            "include_classified",
            "include_boilerplate",
            "include_declarative",
            "include_strings",
            "top_n",
        ],
        "find_dead_code" => &[
            "path",
            "kinds",
            "min_confidence",
            "include_tests",
            "include_classified",
            "top_n",
        ],
        "run_audit" => &[
            "category",
            "min_severity",
            "max_findings",
            "include_classified",
            "base_ref",
            "changed_files",
            "debt",
        ],
        "coverage_map" => &["top_n", "path"],
        "hot_path_review" => &[
            "path",
            "top_n",
            "min_score",
            "cyclomatic_threshold",
            "call_threshold",
            "loop_depth_threshold",
            "time_rank_threshold",
            "include_tests",
            "include_classified",
        ],
        "get_community" => &["max_nodes", "cursor"],
        "list_communities" | "module_map" => &["top_n", "include_non_product"],
        "list_endpoints" => &["method", "path", "max_results", "include_classified"],
        "trace_endpoint" => &[
            "method",
            "handler_file",
            "max_depth",
            "max_nodes",
            "max_excerpts",
            "context_lines",
            "include_classified",
        ],
        "rebuild_graph" => &["mode", "precision", "scope"],
        "graph_diff" => &["base_ref", "path"],
        "get_architecture_contract" => &[
            "action",
            "candidate_contract",
            "baseline_mode",
            "confirm_token",
        ],
        "prepare_change" => &["intent"],
        "propose_architecture_exception" => &["expires"],
        "open_repo" => &["build", "mode", "precision"],
        "semantic_link" => &["model", "min_similarity", "top_k", "selection"],
        "vector_search" => &["top_k", "exact"],
        "seo_link_suggestions" => &[
            "model",
            "min_similarity",
            "top_k",
            "selection",
            "allow_cross_language",
        ],
        _ => &[],
    }
}

pub(super) fn field_schema(tool: &str, name: &str) -> Value {
    if name == "relation_filter" {
        return json!({
            "oneOf": [
                {"type": "string"},
                {"type": "array", "items": {"type": "string"}}
            ]
        });
    }
    if matches!(
        name,
        "clients"
            | "files"
            | "context_filter"
            | "seed_files"
            | "seed_symbols"
            | "changed_files"
            | "client_names"
            | "tests"
            | "kinds"
    ) {
        return json!({"type": "array", "items": {"type": "string"}});
    }
    if matches!(name, "vectors" | "pages" | "events" | "repositories") {
        return json!({"type": "array", "items": {"type": "object"}});
    }
    if tool == "vector_search" && name == "query" {
        return json!({"type": "array", "items": {"type": "number"}});
    }
    if matches!(
        name,
        "request"
            | "candidate_contract"
            | "api_contract"
            | "client_wrappers"
            | "runtime_config"
            | "runtime_evidence_files"
    ) {
        return json!({"type": "object"});
    }
    if matches!(
        name,
        "augment_seeds"
            | "include_classified"
            | "include_low_signal"
            | "include_container_importers"
            | "first_parent"
            | "is_regex"
            | "include_tests"
            | "include_boilerplate"
            | "include_declarative"
            | "include_strings"
            | "include_non_product"
            | "build"
            | "allow_cross_language"
            | "exact"
            | "run_tests"
            | "duplicate_ratchet"
            | "auto_discover_wrappers"
    ) {
        return json!({"type": "boolean"});
    }
    if name == "min_similarity" {
        return json!({"type": "number", "minimum": 0});
    }
    if is_integer(name) {
        return json!({"type": "integer", "minimum": 0});
    }
    enum_schema(tool, name).unwrap_or_else(|| json!({"type": "string"}))
}

fn is_integer(name: &str) -> bool {
    name.starts_with("max_")
        || name.starts_with("min_")
        || name.ends_with("_ms")
        || matches!(
            name,
            "depth"
                | "top_n"
                | "before"
                | "after"
                | "start_line"
                | "context_lines"
                | "months"
                | "token_budget"
                | "community_id"
                | "impact_depth"
                | "data_flow_depth"
                | "loop_depth_threshold"
                | "runtime_evidence_max_age_hours"
                | "page_size"
                | "per_item_limit"
        )
}

fn enum_schema(tool: &str, name: &str) -> Option<Value> {
    let values = match (tool, name) {
        ("query_graph", "mode") => &["bfs", "dfs"][..],
        ("query_graph", "flow_direction") => &["forward", "backward", "both"],
        ("find_duplicates", "mode") => &["strict", "exact", "renamed", "near_miss"],
        ("semantic_link" | "seo_link_suggestions", "selection") => &["union", "mutual", "directed"],
        ("cross_repo_git", "action") => &["histories", "shared_commits", "diff"],
        ("get_architecture_contract", "action") => &["preview"],
        ("run_audit", "debt") => &["new", "existing", "all"],
        ("verified_change", "phase") => &["plan", "verify"],
        ("trace_api_contract", "transport") => &["all", "http", "graphql", "grpc", "event"],
        ("trace_api_contract" | "get_neighbors", "response_detail") => &["compact", "full"],
        ("open_repo" | "rebuild_graph", "mode") => &["full", "no-tests", "tests-only"],
        _ => return None,
    };
    Some(json!({"type": "string", "enum": values}))
}
