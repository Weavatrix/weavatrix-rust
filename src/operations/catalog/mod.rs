use blazingly_json::{Map, Value, json};
use serde::Serialize;

mod definitions;
mod profile;
mod schema;
mod validation;

pub use profile::ToolProfile;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolDefinition {
    pub name: &'static str,
    pub description: &'static str,
    pub input_schema: Value,
}

#[must_use]
pub fn catalog() -> Vec<ToolDefinition> {
    definitions::SPECS
        .iter()
        .filter(|spec| capability_is_compiled(spec.name))
        .map(|spec| tool(spec.name, spec.description, spec.required))
        .collect()
}

#[must_use]
pub fn catalog_for_profile(profile: ToolProfile) -> Vec<ToolDefinition> {
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
        "change_impact"
            | "git_history"
            | "cross_repo_git"
            | "verified_change"
            | "graph_diff"
            | "select_tests"
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
            "enum": ["text", "json", "structured"],
            "default": "json",
            "description": "text returns the concise text block only; json returns \
                            structured output and mirrors it into text for clients that \
                            read only content; structured drops that mirror, which is the \
                            larger copy, and is safe only where the client reads \
                            structuredContent"
        }),
    )]);
    for name in schema::optional_fields(tool_name) {
        properties.insert((*name).to_owned(), schema::field_schema(tool_name, name));
    }
    for name in required {
        let schema = schema::field_schema(tool_name, name);
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
