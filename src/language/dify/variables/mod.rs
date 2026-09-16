mod assign;
mod scan;

use super::model::{AppRecord, DomainRecord, VariableRecord, node_key, variable_key};
use crate::language::yaml_doc::Node;
use weavatrix_graph::NodeKind;

pub(super) fn collect(record: &mut AppRecord, root: &Node, path: &str, raw: &str) {
    if let Some(vars) = root
        .get("workflow")
        .and_then(|workflow| workflow.get("conversation_variables"))
        .and_then(Node::items)
    {
        for variable in vars {
            let name = variable
                .get("name")
                .or_else(|| {
                    variable
                        .get("selector")
                        .and_then(Node::items)
                        .and_then(|items| items.last())
                })
                .and_then(Node::as_str)
                .unwrap_or("conversation");
            let span = record.span.clone();
            ensure_variable(record, "conversation", name, &span);
        }
    }
    if let Some(vars) = root
        .get("workflow")
        .and_then(|workflow| workflow.get("environment_variables"))
        .and_then(Node::items)
    {
        for variable in vars {
            let name = variable.get("name").and_then(Node::as_str).unwrap_or("env");
            let label = if super::redaction::secret_label(name) {
                "env:redacted".to_owned()
            } else {
                format!("env:{name}")
            };
            record.domains.push(DomainRecord {
                owner: record.key.clone(),
                name: label,
                kind: NodeKind::ConfigKey,
                relation: weavatrix_graph::EdgeKind::Configures,
                span: record.span.clone(),
            });
        }
    }
    let Some(nodes) = root
        .get("workflow")
        .and_then(|workflow| workflow.get("graph"))
        .and_then(|graph| graph.get("nodes"))
        .and_then(Node::items)
    else {
        return;
    };
    for node in nodes {
        let Some(id) = node.get("id").and_then(Node::as_str) else {
            continue;
        };
        let owner = node_key(&record.key, id);
        let data = node.get("data");
        let type_name = data
            .and_then(|data| data.get("type"))
            .and_then(Node::as_str);
        assign::walk_selectors(record, &owner, node, type_name, path, raw);
        if let Some(data) = data {
            scan::walk_executable_scalars(data, &mut |scalar| {
                scan::bind_markers(record, &owner, scalar, path, raw);
                if type_name == Some("template-transform") {
                    scan::bind_template_locals(record, &owner, Some(data), scalar, path, raw);
                }
            });
        }
    }
}

pub(super) fn resolve_selector_target(
    record: &mut AppRecord,
    segments: &[&str],
    span: &weavatrix_graph::SourceSpan,
) -> Option<String> {
    match segments[0] {
        "conversation" | "sys" => {
            let name = segments.get(1).copied().unwrap_or(segments[0]);
            Some(ensure_variable(record, segments[0], name, span))
        }
        _ => record
            .nodes
            .iter()
            .find(|item| item.id == segments[0])
            .map(|item| item.key.clone()),
    }
}

fn ensure_variable(
    record: &mut AppRecord,
    scope: &str,
    name: &str,
    span: &weavatrix_graph::SourceSpan,
) -> String {
    let key = variable_key(&record.key, scope, name);
    if !record.variables.iter().any(|item| item.key == key) {
        record.variables.push(VariableRecord {
            key: key.clone(),
            scope: scope.to_owned(),
            name: name.to_owned(),
            span: span.clone(),
        });
    }
    key
}
