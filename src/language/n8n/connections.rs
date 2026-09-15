use super::locations;
use super::model::{
    DomainRecord, LinkRecord, WorkflowRecord, configured_with, depends_on_output, flows_to,
    handles_error, port_key,
};
use crate::model::Diagnostic;
use blazingly_json::Value;
use std::collections::BTreeMap;
use weavatrix_graph::NodeKind;

pub(super) fn collect(
    workflow: &mut WorkflowRecord,
    connections: Option<&Value>,
    names: &BTreeMap<String, String>,
    path: &str,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let Some(Value::Object(groups)) = connections else {
        return;
    };
    let span = locations::file_span(path);
    for (source_name, group) in groups {
        let Some(source_key) = names.get(source_name) else {
            diagnostics.push(Diagnostic {
                code: "n8n.unknown_source".into(),
                message: format!("connection source {source_name:?} is not a node in this workflow"),
                span: Some(span.clone()),
            });
            continue;
        };
        let Some(Value::Object(kinds)) = Some(group) else {
            continue;
        };
        for (kind, slots) in kinds {
            let Some(slots) = slots.as_array() else {
                continue;
            };
            for (output_index, slot) in slots.iter().enumerate() {
                if slot.is_null() {
                    continue;
                }
                let Some(targets) = slot.as_array() else {
                    continue;
                };
                for target in targets {
                    add_link(
                        workflow,
                        names,
                        source_key,
                        source_name,
                        kind,
                        output_index,
                        target,
                        &span,
                        diagnostics,
                    );
                }
            }
        }
    }
}

fn add_link(
    workflow: &mut WorkflowRecord,
    names: &BTreeMap<String, String>,
    source_key: &str,
    source_name: &str,
    kind: &str,
    output_index: usize,
    target: &Value,
    span: &weavatrix_graph::SourceSpan,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let Some(target_name) = target.get("node").and_then(Value::as_str) else {
        return;
    };
    let Some(target_key) = names.get(target_name) else {
        diagnostics.push(Diagnostic {
            code: "n8n.unknown_target".into(),
            message: format!("connection target {target_name:?} is not a node in this workflow"),
            span: Some(span.clone()),
        });
        return;
    };
    let input_index = target
        .get("index")
        .and_then(Value::as_u64)
        .unwrap_or(0) as usize;
    let target_kind = target.get("type").and_then(Value::as_str).unwrap_or(kind);
    let out_port = port_key(source_key, "out", kind, output_index);
    let in_port = port_key(target_key, "in", target_kind, input_index);
    workflow.links.push(LinkRecord {
        from: out_port,
        to: in_port,
        kind: flows_to(),
        span: span.clone(),
        detail: format!("{source_name}/{kind}:{output_index} -> {target_name}/{target_kind}:{input_index}"),
    });
    workflow.links.push(LinkRecord {
        from: target_key.clone(),
        to: source_key.to_owned(),
        kind: depends_on_output(),
        span: span.clone(),
        detail: format!("uses output {output_index} of {source_name}"),
    });
    if kind == "error" || target_kind == "error" {
        workflow.links.push(LinkRecord {
            from: source_key.to_owned(),
            to: target_key.clone(),
            kind: handles_error(),
            span: span.clone(),
            detail: "configured error output".into(),
        });
    }
    if kind.starts_with("ai_") {
        workflow.domains.push(DomainRecord {
            owner: source_key.to_owned(),
            name: format!("{kind}:{target_name}"),
            kind: NodeKind::custom("n8n.ai").unwrap_or(NodeKind::Service),
            relation: configured_with(),
            span: span.clone(),
        });
    }
}
