//! Field schemas whose meaning a caller cannot infer from the name alone.

use blazingly_json::{Value, json};

/// The documented schema for one field, when its behaviour needs stating.
pub(super) fn documented(tool: &str, name: &str) -> Option<Value> {
    if name == "token_budget" && tool != "context_bundle" {
        return Some(json!({
            "type": "integer",
            "minimum": 1,
            "description": "Approximate output ceiling in tokens (serialized bytes / 4); result arrays are trimmed from the tail to fit and the report states what was dropped"
        }));
    }
    if name == "relation_filter" {
        return Some(json!({
            "oneOf": [
                {"type": "string"},
                {"type": "array", "items": {"type": "string"}}
            ]
        }));
    }
    if tool == "graph_diff" && name == "detail" {
        return Some(json!({
            "type": "string",
            "enum": ["file_pairs", "edges"],
            "default": "file_pairs",
            "description": "Aggregate edge churn by source file, target file, and relation by default; request edges for individual edge provenance"
        }));
    }
    match (tool, name) {
        ("find_dead_code", "min_confidence") => Some(json!({
            "type": "integer",
            "minimum": 0,
            "maximum": 100,
            "description": "Evidence tiers: 25 = whole unreferenced file, 50 = exported symbol nothing reaches, 85 = private symbol nothing references. Bounded static analysis never reaches 100"
        })),
        ("hot_path_review", "min_score") => Some(json!({
            "type": "integer",
            "minimum": 0,
            "description": "Floor on score = complexity_cost x (1 + resolved call fan-in), where complexity_cost = extent lines + 3 x cyclomatic + 10 x loop nesting"
        })),
        ("hot_path_review", "cyclomatic_threshold") => Some(json!({
            "type": "integer",
            "minimum": 0,
            "description": "Only functions with at least this many branch decisions"
        })),
        ("hot_path_review", "call_threshold") => Some(json!({
            "type": "integer",
            "minimum": 0,
            "description": "Only functions with at least this many resolved call sites targeting them"
        })),
        ("hot_path_review", "loop_depth_threshold") => Some(json!({
            "type": "integer",
            "minimum": 0,
            "description": "Only functions whose deepest loop nesting reaches this depth"
        })),
        ("context_bundle" | "inspect_symbol", "max_references") => Some(json!({
            "type": "integer",
            "minimum": 1,
            "maximum": 500,
            "description": "Cap on returned relationship edges (default 50)"
        })),
        ("context_bundle", "token_budget") => Some(json!({
            "type": "integer",
            "minimum": 1,
            "description": "Approximate output ceiling in tokens; relationships and related source trim first and the target symbol's own source is never dropped - a budget below the target itself is an explicit error"
        })),
        ("module_map", "depth") => Some(json!({
            "type": "integer",
            "minimum": 1,
            "maximum": 8,
            "description": "Directory depth that defines one module (default 1: top-level folders)"
        })),
        ("find_duplicates", "include_strings") => Some(json!({
            "type": "boolean",
            "default": false,
            "description": "Also compare multi-line string payloads - inline SQL, templates, embedded scripts - which the code pass sees as a single token"
        })),
        ("find_duplicates", "include_declarative") => Some(json!({
            "type": "boolean",
            "default": true,
            "description": "High-recall by default; false suppresses data-only catalogs but retains model, schema, and contract clones"
        })),
        _ => None,
    }
}
