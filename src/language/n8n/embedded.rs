use super::detect::MAX_CODE_BYTES;
use super::expressions;
use super::locations::{self, StringSite};
use super::model::WorkflowRecord;
use blazingly_json::Value;
use weavatrix_parse::{DeclarationKind, Language, extract};

pub(super) fn collect(
    workflow: &mut WorkflowRecord,
    nodes: &[Value],
    sites: &[StringSite],
    path: &str,
) {
    for node in nodes {
        let type_name = node.get("type").and_then(Value::as_str).unwrap_or("");
        if !type_name.contains("code") && !type_name.ends_with(".code") {
            continue;
        }
        let parameters = node.get("parameters").cloned().unwrap_or(Value::Null);
        let language = if parameters.get("language").and_then(Value::as_str) == Some("python")
            || parameters.get("pythonCode").is_some()
        {
            Language::Python
        } else {
            Language::JavaScript
        };
        let Some(source) = parameters
            .get("jsCode")
            .or_else(|| parameters.get("pythonCode"))
            .and_then(Value::as_str)
        else {
            continue;
        };
        if source.len() > MAX_CODE_BYTES {
            continue;
        }
        let facts = extract(source, language);
        let shadowed = facts
            .declarations
            .iter()
            .any(|declaration| declaration.name == "$" || declaration.name == "$node");
        if shadowed {
            continue;
        }
        let Some(name) = node.get("name").and_then(Value::as_str) else {
            continue;
        };
        let Some(owner) = workflow.nodes.iter().find(|item| item.name == name) else {
            continue;
        };
        let owner_key = owner.key.clone();
        let span = sites
            .iter()
            .find(|site| site.decoded == source)
            .map_or_else(
                || locations::file_span(path),
                |site| locations::span_for(path, "", site.raw_start, site.raw_end),
            );
        expressions::bind_from_source(workflow, &owner_key, source, span);
        let _ = facts
            .declarations
            .iter()
            .any(|declaration| declaration.kind == DeclarationKind::Function);
    }
}
