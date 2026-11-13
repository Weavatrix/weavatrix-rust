use super::descriptions::documented;
use blazingly_json::{Value, json};

pub(crate) fn field_schema(tool: &str, name: &str) -> Value {
    if matches!(tool, "ci_restrictions" | "explain_restriction") && name == "scenario" {
        return json!({
            "type": "object",
            "description": "Optional local scenario. For pull_request, branch means the base branch. No secrets or credentials.",
            "additionalProperties": false,
            "properties": {
                "event": {"type": "string"},
                "branch": {"type": "string"},
                "changed_files": {"type": "array", "items": {"type": "string"}}
            }
        });
    }
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
    if matches!(
        name,
        "request"
            | "candidate_contract"
            | "api_contract"
            | "client_wrappers"
            | "runtime_config"
            | "runtime_evidence_files"
            | "scenario"
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
            | "directed"
            | "build"
            | "allow_cross_language"
            | "exact"
            | "run_tests"
            | "duplicate_ratchet"
            | "auto_discover_wrappers"
    ) {
        return json!({"type": "boolean"});
    }
    if tool == "rebuild_graph" && name == "scope" {
        return json!({
            "type": "string",
            "description": "Repository-relative file or directory. Files outside it are not parsed. Omit it for the whole tree."
        });
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
        ("semantic_link", "vectors") => Some(json!({
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
        ("cross_repo_git", "repositories") => Some(json!({
            "type": "array",
            "description": "Local repositories. A path string, or an object with path and an optional name.",
            "items": {
                "oneOf": [
                    {"type": "string"},
                    {
                        "type": "object",
                        "required": ["path"],
                        "properties": {
                            "path": {"type": "string"},
                            "name": {"type": "string"}
                        }
                    }
                ]
            }
        })),
        _ => None,
    }
}
