use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use weavatrix_graph::{AttributeValue, Edge, Node, NodeKind, SourceSpan};

pub const SNAPSHOT_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Capability {
    pub id: String,
    pub state: CapabilityState,
    pub detail: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CapabilityState {
    Complete,
    Partial,
    Unavailable,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Diagnostic {
    pub code: String,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub span: Option<SourceSpan>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Snapshot {
    pub schema_version: u32,
    pub generator: String,
    pub repository: String,
    pub revision: String,
    pub capabilities: Vec<Capability>,
    pub nodes: Vec<Node>,
    pub edges: Vec<Edge>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub diagnostics: Vec<Diagnostic>,
}

impl Snapshot {
    #[must_use]
    pub fn legacy_value(&self) -> Value {
        let nodes = self.nodes.iter().map(legacy_node).collect::<Vec<_>>();
        let links = self.edges.iter().map(legacy_edge).collect::<Vec<_>>();
        Value::Object(Map::from_iter([
            ("nodes".to_owned(), Value::Array(nodes)),
            ("links".to_owned(), Value::Array(links)),
            (
                "schemaVersion".to_owned(),
                Value::String("weavatrix.rust.legacy.v1".into()),
            ),
            ("edgeTypesV".to_owned(), Value::from(2)),
            ("edgeProvenanceV".to_owned(), Value::from(1)),
            (
                "generator".to_owned(),
                Value::String(self.generator.clone()),
            ),
            (
                "repository".to_owned(),
                Value::String(self.repository.clone()),
            ),
            ("revision".to_owned(), Value::String(self.revision.clone())),
        ]))
    }

    /// Serializes a JavaScript Weavatrix-compatible `{ nodes, links }` graph.
    ///
    /// # Errors
    ///
    /// Returns any JSON serialization error.
    pub fn legacy_json(&self, pretty: bool) -> serde_json::Result<String> {
        if pretty {
            serde_json::to_string_pretty(&self.legacy_value())
        } else {
            serde_json::to_string(&self.legacy_value())
        }
    }
}

fn legacy_node(node: &Node) -> Value {
    let mut out = Map::new();
    out.insert("id".into(), Value::String(node.id.to_string()));
    out.insert("label".into(), Value::String(node.label.clone()));
    out.insert("kind".into(), Value::String(node.kind.as_str().to_owned()));
    out.insert("file_type".into(), Value::String("code".into()));
    if node.kind == NodeKind::File {
        out.insert("source_file".into(), Value::String(node.label.clone()));
    }
    if let Some(language) = &node.language {
        out.insert("language".into(), Value::String(language.clone()));
    }
    if let Some(span) = &node.span {
        out.insert("source_file".into(), Value::String(span.file.clone()));
        out.insert("source_range".into(), legacy_range(span));
        out.insert(
            "source_location".into(),
            Value::String(format!("L{}", span.start.line)),
        );
        out.insert(
            "source_end".into(),
            Value::String(format!("L{}", span.end.line)),
        );
    }
    insert_attributes(&mut out, &node.attributes);
    Value::Object(out)
}

fn legacy_edge(edge: &Edge) -> Value {
    let mut out = Map::new();
    out.insert("source".into(), Value::String(edge.source.to_string()));
    out.insert("target".into(), Value::String(edge.target.to_string()));
    out.insert(
        "relation".into(),
        Value::String(edge.kind.as_str().to_owned()),
    );
    out.insert(
        "provenance".into(),
        Value::String(edge.provenance.evidence.as_str().to_owned()),
    );
    out.insert(
        "confidence".into(),
        Value::String(format!("{:?}", edge.provenance.confidence).to_ascii_lowercase()),
    );
    out.insert(
        "extractor".into(),
        Value::String(edge.provenance.extractor.clone()),
    );
    if let Some(detail) = &edge.provenance.detail {
        out.insert("detail".into(), Value::String(detail.clone()));
    }
    if let Some(span) = &edge.provenance.span {
        out.insert("source_file".into(), Value::String(span.file.clone()));
        out.insert("source_range".into(), legacy_range(span));
        out.insert("line".into(), Value::from(span.start.line));
        out.insert(
            "character".into(),
            Value::from(span.start.column.saturating_sub(1)),
        );
    }
    insert_attributes(&mut out, &edge.attributes);
    Value::Object(out)
}

fn legacy_range(span: &SourceSpan) -> Value {
    Value::Object(Map::from_iter([
        (
            "start".to_owned(),
            legacy_position(span.start.line, span.start.column),
        ),
        (
            "end".to_owned(),
            legacy_position(span.end.line, span.end.column),
        ),
    ]))
}

fn legacy_position(line: u32, column: u32) -> Value {
    Value::Object(Map::from_iter([
        ("line".to_owned(), Value::from(line)),
        (
            "character".to_owned(),
            Value::from(column.saturating_sub(1)),
        ),
    ]))
}

fn insert_attributes(
    out: &mut Map<String, Value>,
    attributes: &std::collections::BTreeMap<String, AttributeValue>,
) {
    for (key, value) in attributes {
        if !out.contains_key(key) {
            out.insert(
                key.clone(),
                serde_json::to_value(value).unwrap_or(Value::Null),
            );
        }
    }
}
