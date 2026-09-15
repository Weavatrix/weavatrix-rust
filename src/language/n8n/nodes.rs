use super::locations;
use super::model::{
    DomainRecord, WorkflowRecord, calls_workflow, configured_with, handles_error, uses_credential,
};
use super::redaction;
use blazingly_json::Value;
use weavatrix_graph::NodeKind;

pub(super) fn collect(workflow: &mut WorkflowRecord, nodes: &[Value], root: &Value, path: &str) {
    let span = locations::file_span(path);
    if let Some(id) = root
        .get("settings")
        .and_then(|settings| settings.get("errorWorkflow"))
        .and_then(Value::as_str)
        .filter(|id| !id.is_empty())
    {
        workflow.domains.push(DomainRecord {
            owner: workflow.key.clone(),
            name: format!("errorWorkflow:{id}"),
            kind: NodeKind::custom("n8n.workflow").unwrap_or(NodeKind::Module),
            relation: handles_error(),
            span: span.clone(),
        });
    }
    for node in nodes {
        let Some(name) = node.get("name").and_then(Value::as_str) else {
            continue;
        };
        let Some(owner) = workflow.nodes.iter().find(|item| item.name == name) else {
            continue;
        };
        let owner_key = owner.key.clone();
        credentials(workflow, node, &owner_key, &span);
        entry(workflow, node, &owner_key, &span);
        http(workflow, node, &owner_key, &span);
        fields(workflow, node, &owner_key, &span);
        subworkflow(workflow, node, &owner_key, &span);
    }
}

fn credentials(
    workflow: &mut WorkflowRecord,
    node: &Value,
    owner: &str,
    span: &weavatrix_graph::SourceSpan,
) {
    let Some(Value::Object(map)) = node.get("credentials") else {
        return;
    };
    for (kind, value) in map {
        let id = value.get("id").and_then(Value::as_str).unwrap_or("");
        let name = value.get("name").and_then(Value::as_str).unwrap_or(kind);
        workflow.domains.push(DomainRecord {
            owner: owner.to_owned(),
            name: if id.is_empty() {
                format!("credential:{name}")
            } else {
                format!("credential:{kind}:{id}")
            },
            kind: NodeKind::ConfigKey,
            relation: uses_credential(),
            span: span.clone(),
        });
    }
}

fn entry(
    workflow: &mut WorkflowRecord,
    node: &Value,
    owner: &str,
    span: &weavatrix_graph::SourceSpan,
) {
    let type_name = node.get("type").and_then(Value::as_str).unwrap_or("");
    let kind = if type_name.ends_with("manualTrigger") {
        "entry:manual"
    } else if type_name.ends_with("webhook") {
        "entry:webhook"
    } else if type_name.contains("schedule") {
        "entry:schedule"
    } else if type_name.ends_with("executeWorkflowTrigger") {
        "entry:executeWorkflow"
    } else if type_name.ends_with("errorTrigger") {
        "entry:error"
    } else {
        return;
    };
    workflow.domains.push(DomainRecord {
        owner: owner.to_owned(),
        name: kind.to_owned(),
        kind: NodeKind::Endpoint,
        relation: configured_with(),
        span: span.clone(),
    });
    if let Some(path) = node
        .get("parameters")
        .and_then(|parameters| parameters.get("path"))
        .and_then(Value::as_str)
    {
        workflow.domains.push(DomainRecord {
            owner: owner.to_owned(),
            name: format!("webhook:{path}"),
            kind: NodeKind::Endpoint,
            relation: configured_with(),
            span: span.clone(),
        });
    }
}

fn http(
    workflow: &mut WorkflowRecord,
    node: &Value,
    owner: &str,
    span: &weavatrix_graph::SourceSpan,
) {
    let type_name = node.get("type").and_then(Value::as_str).unwrap_or("");
    if !type_name.ends_with("httpRequest") && !type_name.ends_with("respondToWebhook") {
        return;
    }
    let parameters = node.get("parameters").cloned().unwrap_or(Value::Null);
    let method = parameters
        .get("method")
        .and_then(Value::as_str)
        .unwrap_or("GET");
    let url = parameters
        .get("url")
        .and_then(Value::as_str)
        .or_else(|| parameters.get("path").and_then(Value::as_str))
        .unwrap_or("");
    if redaction::looks_secret("/parameters/url", url) {
        workflow.domains.push(DomainRecord {
            owner: owner.to_owned(),
            name: format!("http:{method}:redacted"),
            kind: NodeKind::Endpoint,
            relation: configured_with(),
            span: span.clone(),
        });
        return;
    }
    let (static_url, dynamic) = if url.starts_with('=') || url.contains("{{") {
        ("", true)
    } else {
        (url, false)
    };
    workflow.domains.push(DomainRecord {
        owner: owner.to_owned(),
        name: if dynamic {
            format!("http:{method}:dynamic")
        } else {
            format!("http:{method}:{static_url}")
        },
        kind: NodeKind::Endpoint,
        relation: configured_with(),
        span: span.clone(),
    });
}

fn fields(
    workflow: &mut WorkflowRecord,
    node: &Value,
    owner: &str,
    span: &weavatrix_graph::SourceSpan,
) {
    let type_name = node.get("type").and_then(Value::as_str).unwrap_or("");
    if !type_name.ends_with("set") && !type_name.contains("editFields") {
        return;
    }
    let assignments = node
        .get("parameters")
        .and_then(|parameters| parameters.get("assignments"))
        .and_then(|assignments| assignments.get("assignments"))
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    for assignment in assignments {
        let Some(name) = assignment.get("name").and_then(Value::as_str) else {
            continue;
        };
        if redaction::looks_secret(&format!("/assignments/{name}"), name) {
            continue;
        }
        workflow.domains.push(DomainRecord {
            owner: owner.to_owned(),
            name: format!("assigns:{name}"),
            kind: NodeKind::Column,
            relation: configured_with(),
            span: span.clone(),
        });
    }
}

fn subworkflow(
    workflow: &mut WorkflowRecord,
    node: &Value,
    owner: &str,
    span: &weavatrix_graph::SourceSpan,
) {
    let type_name = node.get("type").and_then(Value::as_str).unwrap_or("");
    if !type_name.ends_with("executeWorkflow") {
        return;
    }
    let parameters = node.get("parameters").cloned().unwrap_or(Value::Null);
    let source = parameters
        .get("source")
        .and_then(Value::as_str)
        .unwrap_or("database");
    let wait = parameters
        .get("options")
        .and_then(|options| options.get("waitForSubWorkflow"))
        .and_then(Value::as_bool)
        .unwrap_or(true);
    let target = match source {
        "database" => parameters
            .get("workflowId")
            .and_then(workflow_id)
            .map(|id| {
                workflow.domains.push(DomainRecord {
                    owner: owner.to_owned(),
                    name: format!("workflow_id:{id}"),
                    kind: NodeKind::custom("n8n.workflow").unwrap_or(NodeKind::Module),
                    relation: calls_workflow(),
                    span: span.clone(),
                });
                format!("subworkflow:database:{id}")
            }),
        "localFile" => Some("subworkflow:localFile:unmapped".to_owned()),
        "parameter" => Some("subworkflow:embedded:classified".to_owned()),
        "url" => parameters
            .get("workflowId")
            .and_then(Value::as_str)
            .map(|url| format!("subworkflow:url:{url}")),
        _ => Some("subworkflow:unresolved".to_owned()),
    };
    if let Some(name) = target {
        workflow.domains.push(DomainRecord {
            owner: owner.to_owned(),
            name,
            kind: NodeKind::custom("n8n.workflow").unwrap_or(NodeKind::Module),
            relation: calls_workflow(),
            span: span.clone(),
        });
    }
    if !wait {
        workflow.domains.push(DomainRecord {
            owner: owner.to_owned(),
            name: "subworkflow:no-wait".to_owned(),
            kind: NodeKind::Unknown,
            relation: configured_with(),
            span: span.clone(),
        });
    }
}

fn workflow_id(value: &Value) -> Option<String> {
    match value {
        Value::String(id) if !id.is_empty() && !id.starts_with('=') => Some(id.clone()),
        Value::Object(fields) => fields
            .get("value")
            .and_then(Value::as_str)
            .filter(|id| !id.is_empty() && !id.starts_with('='))
            .map(str::to_owned),
        _ => None,
    }
}
