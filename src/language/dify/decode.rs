use super::code;
use super::detect::{self, MAX_EDGES, MAX_FILE_BYTES, MAX_NODES, Mode};
use super::flow;
use super::model::{AppRecord, Coverage, DomainBatch, DomainRecord, NodeRecord, app_key, node_key};
use super::variables;
use crate::language::yaml_doc::{self, Node};
use crate::model::Diagnostic;
use weavatrix_graph::NodeKind;

pub(super) fn decode(path: &str, raw: &str, documents: &[Node]) -> Option<DomainBatch> {
    let exports = documents
        .iter()
        .filter(|document| detect::is_export(document))
        .collect::<Vec<_>>();
    if exports.is_empty() {
        return None;
    }
    if u64::try_from(raw.len()).unwrap_or(u64::MAX) > MAX_FILE_BYTES {
        return Some(limit(
            path,
            format!("Dify YAML exceeds {MAX_FILE_BYTES} bytes"),
        ));
    }
    let mut batch = DomainBatch {
        apps: Vec::new(),
        diagnostics: Vec::new(),
        truncated: false,
    };
    for document in exports {
        match app(path, raw, document) {
            Ok(app) => batch.apps.push(app),
            Err(diagnostic) => {
                batch.truncated = true;
                batch.diagnostics.push(diagnostic);
            }
        }
    }
    Some(batch)
}

fn app(path: &str, raw: &str, root: &Node) -> Result<AppRecord, Diagnostic> {
    let envelope = root
        .get("app")
        .ok_or_else(|| diagnostic(path, raw, root, "missing app envelope"))?;
    let name = envelope
        .get("name")
        .and_then(Node::as_str)
        .unwrap_or("untitled")
        .to_owned();
    let mode = Mode::parse(envelope.get("mode").and_then(Node::as_str).unwrap_or(""));
    let dsl_version = root
        .get("version")
        .and_then(Node::as_str)
        .unwrap_or("")
        .to_owned();
    let key = app_key(path);
    let span = yaml_doc::span_for(path, raw, root.range().0, root.range().1);
    let mut record = AppRecord {
        key: key.clone(),
        name,
        mode: mode.as_str().to_owned(),
        dsl_version: dsl_version.clone(),
        supported: mode.supported() && detect::known_dsl(&dsl_version),
        span: span.clone(),
        nodes: Vec::new(),
        links: Vec::new(),
        domains: Vec::new(),
        coverage: Coverage::default(),
    };
    if !mode.supported() {
        record.domains.push(domain(
            &key,
            format!("mode:{}", mode.as_str()),
            "semantics:structure_only",
            span.clone(),
        ));
    }
    if !detect::known_dsl(&dsl_version) {
        record.domains.push(domain(
            &key,
            format!("dsl:{dsl_version}"),
            "semantics:unsupported",
            span.clone(),
        ));
    }
    let Some(graph) = root
        .get("workflow")
        .and_then(|workflow| workflow.get("graph"))
    else {
        return Ok(record);
    };
    let nodes = graph.get("nodes").and_then(Node::items).unwrap_or(&[]);
    if nodes.len() > MAX_NODES {
        return Err(diagnostic(
            path,
            raw,
            root,
            format!("Dify node count exceeds {MAX_NODES}"),
        ));
    }
    for node in nodes {
        record.nodes.push(node_record(&key, path, raw, node));
    }
    let edges = graph.get("edges").and_then(Node::items).unwrap_or(&[]);
    if edges.len() > MAX_EDGES {
        return Err(diagnostic(
            path,
            raw,
            root,
            format!("Dify edge count exceeds {MAX_EDGES}"),
        ));
    }
    flow::collect(&mut record, edges, path, raw);
    variables::collect(&mut record, root, path, raw);
    code::collect(&mut record, nodes, path, raw);
    super::plugins::collect(&mut record, root, nodes, path, raw);
    record.coverage = coverage_of(&record);
    Ok(record)
}

fn node_record(app: &str, path: &str, raw: &str, node: &Node) -> NodeRecord {
    let id = node
        .get("id")
        .and_then(Node::as_str)
        .unwrap_or("")
        .to_owned();
    let data = node.get("data");
    let type_name = data
        .and_then(|data| data.get("type"))
        .and_then(Node::as_str)
        .unwrap_or("custom")
        .to_owned();
    let title = data
        .and_then(|data| data.get("title"))
        .and_then(Node::as_str)
        .unwrap_or("")
        .to_owned();
    let parent = node
        .get("parentId")
        .and_then(Node::as_str)
        .map(str::to_owned)
        .or_else(|| {
            data.and_then(|data| data.get("iteration_id"))
                .and_then(Node::as_str)
                .map(str::to_owned)
        });
    let (start, end) = node.range();
    NodeRecord {
        key: node_key(app, &id),
        id,
        title,
        type_name,
        parent,
        span: yaml_doc::span_for(path, raw, start, end),
    }
}

fn coverage_of(record: &AppRecord) -> Coverage {
    let mut coverage = record.coverage.clone();
    coverage.structure_seen = u32::try_from(record.nodes.len()).unwrap_or(u32::MAX);
    coverage.structure_accepted = coverage.structure_seen;
    coverage.semantics_seen = coverage.structure_seen;
    coverage.semantics_supported = u32::try_from(
        record
            .nodes
            .iter()
            .filter(|node| detect::known_node(&node.type_name))
            .count(),
    )
    .unwrap_or(u32::MAX);
    coverage.locations_seen = coverage.structure_seen;
    coverage.locations_exact = coverage.structure_seen;
    coverage
}

fn domain(
    owner: &str,
    name: impl Into<String>,
    relation: &str,
    span: weavatrix_graph::SourceSpan,
) -> DomainRecord {
    let relation = match relation {
        "uses_model" => super::model::uses_model(),
        "invokes_tool" => super::model::invokes_tool(),
        "queries_knowledge" => super::model::queries_knowledge(),
        "imports_plugin" => super::model::imports_plugin(),
        _ => weavatrix_graph::EdgeKind::Configures,
    };
    DomainRecord {
        owner: owner.to_owned(),
        name: name.into(),
        kind: NodeKind::Unknown,
        relation,
        span,
    }
}

fn span(path: &str, raw: &str, node: &Node) -> weavatrix_graph::SourceSpan {
    let (start, end) = node.range();
    yaml_doc::span_for(path, raw, start, end)
}

fn diagnostic(path: &str, raw: &str, node: &Node, message: impl Into<String>) -> Diagnostic {
    Diagnostic {
        code: "dify.limit".into(),
        message: message.into(),
        span: Some(span(path, raw, node)),
    }
}

fn limit(path: &str, message: String) -> DomainBatch {
    DomainBatch {
        apps: Vec::new(),
        diagnostics: vec![Diagnostic {
            code: "dify.limit".into(),
            message,
            span: Some(yaml_doc::span_for(path, "", 0, 0)),
        }],
        truncated: true,
    }
}
