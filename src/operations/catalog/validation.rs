use blazingly_json::{Value, json};
use std::collections::BTreeSet;

/// Shared keys every tool accepts beside its own contract.
pub(crate) const SHARED_ARGUMENT_KEYS: &[&str] = &["output_format", "expected_repository"];

/// Rejects tool-specific keys that are not advertised for `tool`.
///
/// Shared keys always pass. MCP hosts may strip extras before delivery; this
/// still fails at call time when an unknown key reaches the engine.
pub(crate) fn reject_unknown_arguments(
    tool: &str,
    args: &Value,
    tool_keys: &[&str],
) -> Result<(), String> {
    let Some(object) = args.as_object() else {
        return Ok(());
    };
    let supported = tool_keys
        .iter()
        .chain(SHARED_ARGUMENT_KEYS.iter())
        .copied()
        .collect::<BTreeSet<_>>();
    for key in object.keys() {
        if supported.contains(key.as_str()) {
            continue;
        }
        let listed = supported.into_iter().collect::<Vec<_>>().join(", ");
        return Err(format!(
            "unknown argument `{key}` for {tool}; supported: {listed}"
        ));
    }
    Ok(())
}

/// Validates `output_format` when present. Invalid values must fail rather than
/// be ignored by hosts that only read the catalog enum.
pub(crate) fn require_valid_output_format(args: &Value) -> Result<(), String> {
    let Some(value) = args.get("output_format") else {
        return Ok(());
    };
    let Some(format) = value.as_str() else {
        return Err("output_format must be a string".to_owned());
    };
    if matches!(format, "text" | "json" | "structured") {
        return Ok(());
    }
    Err(format!(
        "invalid output_format {format:?}; expected text, json, or structured"
    ))
}

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
                | "line"
                | "column"
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
        ("run_audit", "category") => &[
            "all",
            "diagnostics",
            "structure",
            "dependencies",
            "runtime",
            "tests",
        ],
        ("run_audit", "min_severity") => &["low", "medium", "high", "critical"],
        ("verified_change", "phase") => &["plan", "verify"],
        ("perf_attribution", "direction") => &["lower_is_better", "higher_is_better"],
        ("get_dependents" | "change_impact" | "select_tests", "precision") => &["graph"],
        ("trace_api_contract", "transport") => &["all", "http", "graphql", "grpc", "event"],
        ("trace_api_contract" | "get_neighbors", "response_detail") => &["compact", "full"],
        ("open_repo" | "rebuild_graph", "mode") => &["full", "no-tests", "tests-only"],
        _ => return None,
    };
    Some(json!({"type": "string", "enum": values}))
}
