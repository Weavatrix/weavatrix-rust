use blazingly_json::{Value, json};
use super::descriptions::documented;

pub(crate) fn field_schema(tool: &str, name: &str) -> Value {
    if let Some(documented) = documented(tool, name) {
        return documented;
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
        return documented_object_array(tool, name)
            .unwrap_or_else(|| json!({"type": "array", "items": {"type": "object"}}));
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
            | "include_capabilities"
            | "include_analytics"
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
    if let Some(schema) = super::super::validation::enum_schema(tool, name) {
        return schema;
    }
    if super::super::validation::is_integer(name) {
        return json!({"type": "integer", "minimum": 0});
    }
    json!({"type": "string"})
}

fn documented_object_array(tool: &str, name: &str) -> Option<Value> {
    match (tool, name) {
        ("semantic_link" | "vector_search" | "seo_link_suggestions", "vectors") => Some(json!({
            "type": "array",
            "description": "Caller-supplied embedding rows. Each object requires node (string graph id) and values (array of numbers). Do not send id/vector.",
            "items": {
                "type": "object",
                "required": ["node", "values"],
                "properties": {
                    "node": {"type": "string"},
                    "values": {"type": "array", "items": {"type": "number"}}
                }
            }
        })),
        ("seo_link_suggestions", "pages") => Some(json!({
            "type": "array",
            "description": "Page profiles for SEO linking. Each object requires node, site, and canonical; language and title are optional.",
            "items": {
                "type": "object",
                "required": ["node", "site", "canonical"],
                "properties": {
                    "node": {"type": "string"},
                    "site": {"type": "string"},
                    "canonical": {"type": "string"},
                    "language": {"type": "string"},
                    "title": {"type": "string"}
                }
            }
        })),
        ("memory_context", "events") => Some(json!({
            "type": "array",
            "description": "StoredEvent-shaped rows (not empty objects). Each event needs metadata with at least id, stream_id, stream_version, global_position, event_type, occurred_at, recorded_at, agent_id, and session_id, plus a typed payload.",
            "items": {
                "type": "object",
                "required": ["metadata", "payload"],
                "properties": {
                    "metadata": {"type": "object"},
                    "payload": {"type": "object"}
                }
            }
        })),
        _ => None,
    }
}
