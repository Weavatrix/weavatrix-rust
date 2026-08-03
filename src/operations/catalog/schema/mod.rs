//! JSON schemas for the operation catalog.

mod optional_sections;

use blazingly_json::{Value, json};
use optional_sections::{extension_fields, health_fields};

pub(super) fn optional_fields(tool: &str) -> &'static [&'static str] {
    graph_fields(tool)
        .or_else(|| change_fields(tool))
        .or_else(|| source_and_api_fields(tool))
        .or_else(|| health_fields(tool))
        .or_else(|| extension_fields(tool))
        .unwrap_or(&[])
}

fn graph_fields(tool: &str) -> Option<&'static [&'static str]> {
    match tool {
        "get_neighbors" => Some(&[
            "relation_filter",
            "max_results",
            "cursor",
            "response_detail",
        ]),
        "query_graph" => Some(&[
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
        ]),
        "god_nodes" => Some(&["top_n", "include_classified"]),
        "shortest_path" => Some(&["max_hops"]),
        "get_community" => Some(&["max_nodes", "cursor"]),
        "list_communities" | "module_map" => Some(&["top_n", "include_non_product"]),
        "build_graph" => Some(&["max_members"]),
        _ => None,
    }
}

fn change_fields(tool: &str) -> Option<&'static [&'static str]> {
    match tool {
        "get_dependents" => Some(&[
            "depth",
            "max_nodes",
            "precision",
            "max_references",
            "timeout_ms",
            "include_container_importers",
        ]),
        "change_impact" => Some(&[
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
        ]),
        "git_history" => Some(&[
            "revision",
            "months",
            "max_commits",
            "min_pair_count",
            "max_pairs",
            "top_n",
            "first_parent",
        ]),
        "cross_repo_git" => Some(&[
            "action",
            "revision",
            "base_ref",
            "head_ref",
            "max_commits",
            "first_parent",
            "left",
            "right",
        ]),
        "verified_change" => Some(&[
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
        ]),
        "select_tests" => Some(&[
            "base",
            "base_ref",
            "head_ref",
            "diff",
            "files",
            "depth",
            "max_nodes",
            "max_tests",
            "precision",
        ]),
        "graph_diff" => Some(&["base_ref", "path"]),
        "prepare_change" => Some(&["intent"]),
        _ => None,
    }
}

fn source_and_api_fields(tool: &str) -> Option<&'static [&'static str]> {
    match tool {
        "trace_api_contract" => Some(&[
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
        ]),
        "map_stacktrace" => Some(&["max_frames"]),
        "search_code" => Some(&[
            "is_regex",
            "glob",
            "before",
            "after",
            "max_results",
            "token_budget",
        ]),
        "read_source" => Some(&["label", "path", "start_line", "before", "after", "token_budget"]),
        "inspect_symbol" => Some(&[
            "precision",
            "max_references",
            "max_containers",
            "context_lines",
            "timeout_ms",
        ]),
        "context_bundle" => Some(&[
            "precision",
            "max_references",
            "max_related",
            "max_reexports",
            "max_source_files",
            "context_lines",
            "include_classified",
            "timeout_ms",
            "token_budget",
        ]),
        "list_endpoints" => Some(&["method", "path", "max_results", "include_classified"]),
        "trace_endpoint" => Some(&[
            "method",
            "handler_file",
            "max_depth",
            "max_nodes",
            "max_excerpts",
            "context_lines",
            "include_classified",
        ]),
        _ => None,
    }
}

pub(super) fn field_schema(tool: &str, name: &str) -> Value {
    if name == "token_budget" {
        return json!({
            "type": "integer",
            "minimum": 1,
            "description": "Approximate output ceiling in tokens (serialized bytes / 4); result arrays are trimmed from the tail to fit and the report states what was dropped"
        });
    }
    if name == "relation_filter" {
        return json!({
            "oneOf": [
                {"type": "string"},
                {"type": "array", "items": {"type": "string"}}
            ]
        });
    }
    if tool == "find_duplicates" && name == "include_declarative" {
        return json!({
            "type": "boolean",
            "default": true,
            "description": "High-recall by default; false suppresses data-only catalogs but retains model, schema, and contract clones"
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
        return json!({
            "type": "number",
            "minimum": 0,
            "maximum": 100,
            "description": "0..1 is a fraction; values above 1 through 100 are percentages"
        });
    }
    if name == "min_confidence" {
        return json!({"type": "integer", "minimum": 0, "maximum": 100});
    }
    if let Some(schema) = super::validation::enum_schema(tool, name) {
        return schema;
    }
    if super::validation::is_integer(name) {
        return json!({"type": "integer", "minimum": 0});
    }
    json!({"type": "string"})
}
