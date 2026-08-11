//! Field schemas whose meaning a caller cannot infer from the name alone.

use blazingly_json::{Value, json};

/// The documented schema for one field, when its behaviour needs stating.
pub(super) fn documented(tool: &str, name: &str) -> Option<Value> {
    if name == "token_budget" {
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
    if tool != "find_duplicates" {
        return None;
    }
    match name {
        "include_strings" => Some(json!({
            "type": "boolean",
            "default": false,
            "description": "Also compare multi-line string payloads - inline SQL, templates, embedded scripts - which the code pass sees as a single token"
        })),
        "include_declarative" => Some(json!({
            "type": "boolean",
            "default": true,
            "description": "High-recall by default; false suppresses data-only catalogs but retains model, schema, and contract clones"
        })),
        _ => None,
    }
}
