use blazingly_json::{Value, json};

pub(super) fn is_integer(name: &str) -> bool {
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

pub(super) fn enum_schema(tool: &str, name: &str) -> Option<Value> {
    let values = match (tool, name) {
        ("query_graph", "mode") => &["bfs", "dfs"][..],
        ("query_graph", "flow_direction") => &["forward", "backward", "both"],
        ("find_duplicates", "mode") => &["strict", "exact", "renamed", "near_miss"],
        ("semantic_link" | "seo_link_suggestions", "selection") => &["union", "mutual", "directed"],
        ("cross_repo_git", "action") => &["histories", "shared_commits", "diff"],
        ("get_architecture_contract", "action") => &["preview"],
        ("run_audit", "debt") => &["new", "existing", "all"],
        ("run_audit", "category") => {
            &["all", "diagnostics", "structure", "dependencies", "runtime"]
        }
        ("run_audit", "min_severity") => &["low", "medium", "high", "critical"],
        ("verified_change", "phase") => &["plan", "verify"],
        ("get_dependents" | "change_impact", "precision") => &["graph"],
        ("trace_api_contract", "transport") => &["all", "http", "graphql", "grpc", "event"],
        ("trace_api_contract" | "get_neighbors", "response_detail") => &["compact", "full"],
        ("open_repo" | "rebuild_graph", "mode") => &["full", "no-tests", "tests-only"],
        _ => return None,
    };
    Some(json!({"type": "string", "enum": values}))
}
