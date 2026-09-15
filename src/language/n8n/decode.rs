use super::connections;
use super::detect::{
    self, Completeness, MAX_CONNECTIONS, MAX_NODES, MAX_WORKFLOW_BYTES, completeness,
    has_n8n_shape, is_workflow,
};
use super::embedded;
use super::expressions;
use super::locations::{self, StringSite};
use super::model::{DomainBatch, NodeRecord, WorkflowRecord, node_key, workflow_key};
use super::nodes;
use crate::model::Diagnostic;
use blazingly_json::Value;

pub(super) fn decode(path: &str, raw: &str, value: &Value) -> Option<DomainBatch> {
    let documents = match value {
        Value::Array(items) => items
            .iter()
            .enumerate()
            .filter(|(_, item)| is_workflow(item))
            .map(|(index, item)| (Some(index), item))
            .collect::<Vec<_>>(),
        Value::Object(_) if is_workflow(value) => vec![(None, value)],
        _ => Vec::new(),
    };
    if documents.is_empty() {
        if has_n8n_shape(value) {
            return Some(DomainBatch {
                workflows: Vec::new(),
                diagnostics: vec![Diagnostic {
                    code: "n8n.invalid_workflow".into(),
                    message: "n8n-shaped JSON has no structurally valid nodes".into(),
                    span: Some(locations::file_span(path)),
                }],
                truncated: false,
                coverage: super::model::Coverage::default(),
            });
        }
        return None;
    }
    let size = u64::try_from(raw.len()).unwrap_or(u64::MAX);
    if size > MAX_WORKFLOW_BYTES {
        return Some(limit_batch(
            path,
            format!("n8n workflow JSON exceeds {MAX_WORKFLOW_BYTES} bytes"),
        ));
    }
    let sites = locations::string_sites(raw);
    let mut batch = DomainBatch {
        workflows: Vec::new(),
        diagnostics: Vec::new(),
        truncated: false,
        coverage: super::model::Coverage::default(),
    };
    for (index, document) in documents {
        match workflow(path, raw, document, index, &sites, &mut batch.diagnostics) {
            Ok(workflow) => {
                batch.coverage.merge(&workflow.coverage);
                batch.workflows.push(workflow);
            }
            Err(message) => {
                batch.truncated = true;
                batch.diagnostics.push(limit_diagnostic(path, message));
            }
        }
    }
    Some(batch)
}

fn workflow(
    path: &str,
    raw: &str,
    value: &Value,
    index: Option<usize>,
    sites: &[StringSite],
    diagnostics: &mut Vec<Diagnostic>,
) -> Result<WorkflowRecord, String> {
    let nodes = value
        .get("nodes")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    if nodes.len() > MAX_NODES {
        return Err(format!(
            "n8n node count {} exceeds {MAX_NODES}",
            nodes.len()
        ));
    }
    let name = value
        .get("name")
        .and_then(Value::as_str)
        .unwrap_or("untitled")
        .to_owned();
    let id = value.get("id").and_then(Value::as_str).map(str::to_owned);
    let namespace = index.map_or_else(|| path.to_owned(), |index| format!("{path}#{index}"));
    let key = workflow_key(&namespace, id.as_deref(), &name);
    let span = locations::file_span(path);
    let mut record = WorkflowRecord {
        key: key.clone(),
        name,
        id,
        completeness: completeness(value),
        span: span.clone(),
        nodes: Vec::new(),
        links: Vec::new(),
        domains: Vec::new(),
        coverage: super::model::Coverage::default(),
    };
    for (ordinal, node) in nodes.iter().enumerate() {
        if !detect::is_node(node) {
            diagnostics.push(Diagnostic {
                code: "n8n.invalid_node".into(),
                message: format!("unrecognized node at index {ordinal}"),
                span: Some(span.clone()),
            });
            continue;
        }
        record
            .nodes
            .push(node_record(&key, node, path, raw, sites, ordinal));
    }
    let names = record
        .nodes
        .iter()
        .map(|node| (node.name.clone(), node.key.clone()))
        .collect::<std::collections::BTreeMap<_, _>>();
    connections::collect(
        &mut record,
        value.get("connections"),
        &names,
        path,
        diagnostics,
    );
    if connection_count(&record) > MAX_CONNECTIONS {
        return Err(format!("n8n connection count exceeds {MAX_CONNECTIONS}"));
    }
    expressions::collect(&mut record, &nodes, sites, path, raw);
    embedded::collect(&mut record, &nodes, sites, path, raw);
    nodes::collect(&mut record, &nodes, value, path);
    record.coverage = coverage_of(&record);
    if record.completeness == Completeness::Partial {
        diagnostics.push(Diagnostic {
            code: "n8n.partial".into(),
            message: "clipboard or partial export; version identity is limited".into(),
            span: Some(span),
        });
    }
    Ok(record)
}

fn node_record(
    workflow_key: &str,
    node: &Value,
    path: &str,
    raw: &str,
    sites: &[StringSite],
    ordinal: usize,
) -> NodeRecord {
    let name = node
        .get("name")
        .and_then(Value::as_str)
        .unwrap_or("node")
        .to_owned();
    let id = node
        .get("id")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_owned();
    let type_name = node
        .get("type")
        .and_then(Value::as_str)
        .unwrap_or("unknown")
        .to_owned();
    let type_version = type_version(node);
    let semantics = super::families::semantics(&type_name, &type_version);
    NodeRecord {
        key: node_key(workflow_key, &id, &name),
        id,
        name: name.clone(),
        type_name,
        type_version,
        semantics,
        span: node_span(path, raw, sites, ordinal),
    }
}

fn node_span(
    path: &str,
    raw: &str,
    sites: &[StringSite],
    ordinal: usize,
) -> weavatrix_graph::SourceSpan {
    let suffix = format!("/nodes/{ordinal}/name");
    if let Some(site) = sites.iter().find(|site| site.pointer.ends_with(&suffix)) {
        return locations::span_for(path, raw, site.raw_start, site.raw_end);
    }
    let column = u32::try_from(ordinal.saturating_add(2)).unwrap_or(u32::MAX);
    weavatrix_graph::SourceSpan::new(
        path,
        weavatrix_graph::SourcePosition::new(1, column),
        weavatrix_graph::SourcePosition::new(1, column.saturating_add(1)),
    )
}

fn coverage_of(record: &WorkflowRecord) -> super::model::Coverage {
    let mut coverage = record.coverage.clone();
    coverage.structure_seen = u32::try_from(record.nodes.len()).unwrap_or(u32::MAX);
    coverage.structure_accepted = coverage.structure_seen;
    coverage.semantics_seen = coverage.structure_seen;
    coverage.semantics_supported = u32::try_from(
        record
            .nodes
            .iter()
            .filter(|node| node.semantics == super::model::NodeSemantics::Supported)
            .count(),
    )
    .unwrap_or(u32::MAX);
    coverage
}

fn type_version(node: &Value) -> String {
    match node.get("typeVersion") {
        Some(Value::Number(number)) => number.to_string(),
        Some(Value::String(value)) => value.clone(),
        _ => "1".to_owned(),
    }
}

fn connection_count(record: &WorkflowRecord) -> usize {
    record
        .links
        .iter()
        .filter(|link| link.kind == super::model::flows_to())
        .count()
}

fn limit_batch(path: &str, message: String) -> DomainBatch {
    DomainBatch {
        workflows: Vec::new(),
        diagnostics: vec![limit_diagnostic(path, message)],
        truncated: true,
        coverage: super::model::Coverage::default(),
    }
}

fn limit_diagnostic(path: &str, message: String) -> Diagnostic {
    Diagnostic {
        code: "n8n.limit".into(),
        message,
        span: Some(locations::file_span(path)),
    }
}
