use blazingly_json::Value;

pub(super) const MAX_WORKFLOW_BYTES: u64 = 16 * 1024 * 1024;
pub(super) const MAX_NODES: usize = 5_000;
pub(super) const MAX_CONNECTIONS: usize = 30_000;
pub(super) const MAX_EXPRESSION_BYTES: usize = 64 * 1024;
pub(super) const MAX_CODE_BYTES: usize = 1024 * 1024;
pub(crate) const DEFAULT_FILE_BYTES: u64 = 1_500_000;

/// Cheap prefix probe so large non-n8n JSON is not admitted as workflow source.
#[must_use]
pub(crate) fn looks_promising(text: &str) -> bool {
    let head = text.get(..8192).unwrap_or(text);
    head.contains("\"nodes\"") && head.contains("\"connections\"") && head.contains("\"type\"")
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Completeness {
    Full,
    Partial,
}

#[must_use]
pub(super) fn completeness(value: &Value) -> Completeness {
    let has_identity = value.get("id").and_then(Value::as_str).is_some()
        || value.get("versionId").and_then(Value::as_str).is_some();
    let has_name = value.get("name").and_then(Value::as_str).is_some();
    if has_identity && has_name {
        Completeness::Full
    } else {
        Completeness::Partial
    }
}

/// Shape that looks like an n8n export even when no node is structurally valid.
#[must_use]
pub(super) fn has_n8n_shape(value: &Value) -> bool {
    match value {
        Value::Array(items) => items.iter().any(has_n8n_shape),
        Value::Object(_) => {
            value.get("nodes").is_some_and(Value::is_array)
                && value.get("connections").is_some_and(Value::is_object)
        }
        _ => false,
    }
}

/// Structure-first recognition. `nodes` + `connections` alone is not enough.
#[must_use]
pub(super) fn is_workflow(value: &Value) -> bool {
    let Some(nodes) = value.get("nodes").and_then(Value::as_array) else {
        return false;
    };
    let Some(connections) = value.get("connections") else {
        return false;
    };
    if !connections.is_object() {
        return false;
    }
    !nodes.is_empty() && nodes.iter().any(is_node)
}

#[must_use]
pub(super) fn is_node(value: &Value) -> bool {
    value
        .get("type")
        .and_then(Value::as_str)
        .is_some_and(|kind| !kind.is_empty())
        && value
            .get("name")
            .and_then(Value::as_str)
            .is_some_and(|name| !name.is_empty())
        && value
            .get("parameters")
            .is_none_or(|parameters| parameters.is_object() || parameters.is_null())
}

#[cfg(test)]
mod tests {
    use super::{completeness, is_workflow};
    use crate::language::n8n::detect::Completeness;
    use blazingly_json::json;

    #[test]
    fn foreign_nodes_array_is_rejected() {
        let value = json!({
            "nodes": [{"id": 1}],
            "connections": {"left": "right"}
        });
        assert!(!is_workflow(&value));
    }

    #[test]
    fn clipboard_without_ids_is_partial() {
        let value = json!({
            "nodes": [{
                "name": "Start",
                "type": "n8n-nodes-base.manualTrigger",
                "parameters": {}
            }],
            "connections": {}
        });
        assert!(is_workflow(&value));
        assert_eq!(completeness(&value), Completeness::Partial);
    }
}
