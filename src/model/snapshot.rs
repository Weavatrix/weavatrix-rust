use blazingly_json::{Map, Value};
use serde::{Deserialize, Serialize};
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
    pub fn legacy_json(&self, pretty: bool) -> blazingly_json::Result<String> {
        if pretty {
            blazingly_json::to_string_pretty(&self.legacy_value())
        } else {
            blazingly_json::to_string(&self.legacy_value())
        }
    }
}

fn legacy_node(node: &Node) -> Value {
    let mut out = legacy_object([
        ("id", Value::String(node.id.to_string())),
        ("label", Value::String(node.label.clone())),
        ("kind", Value::String(node.kind.as_str().to_owned())),
        ("file_type", Value::String("code".into())),
    ]);
    if node.kind == NodeKind::File {
        out.insert("source_file".into(), Value::String(node.label.clone()));
    }
    if let Some(language) = &node.language {
        out.insert("language".into(), Value::String(language.clone()));
    }
    legacy_record(out, node.span.as_ref(), &node.attributes, |out, span| {
        out.insert(
            "source_location".into(),
            Value::String(format!("L{}", span.start.line)),
        );
        out.insert(
            "source_end".into(),
            Value::String(format!("L{}", span.end.line)),
        );
    })
}

fn legacy_edge(edge: &Edge) -> Value {
    let mut out = legacy_object([
        ("source", Value::String(edge.source.to_string())),
        ("target", Value::String(edge.target.to_string())),
        ("relation", Value::String(edge.kind.as_str().to_owned())),
        (
            "provenance",
            Value::String(edge.provenance.evidence.as_str().to_owned()),
        ),
        (
            "confidence",
            Value::String(format!("{:?}", edge.provenance.confidence).to_ascii_lowercase()),
        ),
        (
            "extractor",
            Value::String(edge.provenance.extractor.clone()),
        ),
    ]);
    if let Some(detail) = &edge.provenance.detail {
        out.insert("detail".into(), Value::String(detail.clone()));
    }
    legacy_record(
        out,
        edge.provenance.span.as_ref(),
        &edge.attributes,
        |out, span| {
            out.insert("line".into(), Value::from(span.start.line));
            out.insert(
                "character".into(),
                Value::from(span.start.column.saturating_sub(1)),
            );
        },
    )
}

fn legacy_object(entries: impl IntoIterator<Item = (&'static str, Value)>) -> Map<String, Value> {
    let mut out = Map::new();
    for (key, value) in entries {
        out.insert(key.to_owned(), value);
    }
    out
}

fn legacy_record(
    mut out: Map<String, Value>,
    span: Option<&SourceSpan>,
    attributes: &std::collections::BTreeMap<String, AttributeValue>,
    decorate_span: impl FnOnce(&mut Map<String, Value>, &SourceSpan),
) -> Value {
    if let Some(span) = span {
        out.insert("source_file".into(), Value::String(span.file.clone()));
        out.insert("source_range".into(), legacy_range(span));
        decorate_span(&mut out, span);
    }
    insert_attributes(&mut out, attributes);
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
                blazingly_json::to_value(value).unwrap_or(Value::Null),
            );
        }
    }
}
