use super::detect::Completeness;
use crate::model::Diagnostic;
use weavatrix_graph::{EdgeKind, NodeKind, SourceSpan};

#[derive(Debug, Clone, Default)]
pub(super) struct Coverage {
    pub structure_accepted: u32,
    pub structure_seen: u32,
    pub semantics_supported: u32,
    pub semantics_seen: u32,
    pub expressions_resolved: u32,
    pub expressions_seen: u32,
}

impl Coverage {
    pub(super) fn merge(&mut self, other: &Self) {
        self.structure_accepted += other.structure_accepted;
        self.structure_seen += other.structure_seen;
        self.semantics_supported += other.semantics_supported;
        self.semantics_seen += other.semantics_seen;
        self.expressions_resolved += other.expressions_resolved;
        self.expressions_seen += other.expressions_seen;
    }
}

#[derive(Debug, Clone)]
pub(super) struct DomainBatch {
    pub workflows: Vec<WorkflowRecord>,
    pub diagnostics: Vec<Diagnostic>,
    pub truncated: bool,
    pub coverage: Coverage,
}

#[derive(Debug, Clone)]
pub(super) struct WorkflowRecord {
    pub key: String,
    pub name: String,
    pub id: Option<String>,
    pub completeness: Completeness,
    pub span: SourceSpan,
    pub nodes: Vec<NodeRecord>,
    pub links: Vec<LinkRecord>,
    pub domains: Vec<DomainRecord>,
    pub coverage: Coverage,
}

#[derive(Debug, Clone)]
pub(super) struct NodeRecord {
    pub key: String,
    pub id: String,
    pub name: String,
    pub type_name: String,
    pub type_version: String,
    pub semantics: NodeSemantics,
    pub span: SourceSpan,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum NodeSemantics {
    Supported,
    Unsupported,
    StructureOnly,
}

impl NodeSemantics {
    pub(super) const fn as_str(self) -> &'static str {
        match self {
            Self::Supported => "supported",
            Self::Unsupported => "unsupported",
            Self::StructureOnly => "structure_only",
        }
    }
}

#[derive(Debug, Clone)]
pub(super) struct LinkRecord {
    pub from: String,
    pub to: String,
    pub kind: EdgeKind,
    pub span: SourceSpan,
    pub detail: String,
}

#[derive(Debug, Clone)]
pub(super) struct DomainRecord {
    pub owner: String,
    pub name: String,
    pub kind: NodeKind,
    pub relation: EdgeKind,
    pub span: SourceSpan,
}

#[must_use]
pub(super) fn workflow_kind() -> NodeKind {
    NodeKind::custom("n8n.workflow").unwrap_or(NodeKind::Module)
}

#[must_use]
pub(super) fn node_kind() -> NodeKind {
    NodeKind::custom("n8n.node").unwrap_or(NodeKind::Function)
}

#[must_use]
pub(super) fn port_kind() -> NodeKind {
    NodeKind::custom("n8n.port").unwrap_or(NodeKind::Binding)
}

#[must_use]
pub(super) fn flows_to() -> EdgeKind {
    EdgeKind::custom("flows_to").unwrap_or(EdgeKind::Calls)
}

#[must_use]
pub(super) fn depends_on_output() -> EdgeKind {
    EdgeKind::custom("depends_on_output").unwrap_or(EdgeKind::DependsOn)
}

#[must_use]
pub(super) fn reads_field() -> EdgeKind {
    EdgeKind::custom("reads_field").unwrap_or(EdgeKind::Reads)
}

#[must_use]
pub(super) fn calls_workflow() -> EdgeKind {
    EdgeKind::custom("calls_workflow").unwrap_or(EdgeKind::Calls)
}

#[must_use]
pub(super) fn handles_error() -> EdgeKind {
    EdgeKind::custom("handles_error_with").unwrap_or(EdgeKind::Binds)
}

#[must_use]
pub(super) fn uses_credential() -> EdgeKind {
    EdgeKind::custom("uses_credential").unwrap_or(EdgeKind::Configures)
}

#[must_use]
pub(super) fn uses_variable() -> EdgeKind {
    EdgeKind::custom("uses_variable").unwrap_or(EdgeKind::Reads)
}

#[must_use]
pub(super) fn configured_with() -> EdgeKind {
    EdgeKind::custom("configured_with").unwrap_or(EdgeKind::Configures)
}

#[must_use]
pub(super) fn workflow_key(namespace: &str, workflow_id: Option<&str>, name: &str) -> String {
    match workflow_id {
        Some(id) if !id.is_empty() => format!("{namespace}::{id}"),
        _ => format!("{namespace}::origin:{name}"),
    }
}

#[must_use]
pub(super) fn node_key(workflow_key: &str, node_id: &str, node_name: &str) -> String {
    if node_id.is_empty() {
        format!("{workflow_key}::{node_name}")
    } else {
        format!("{workflow_key}::{node_id}")
    }
}

#[must_use]
pub(super) fn port_key(node_key: &str, direction: &str, kind: &str, index: usize) -> String {
    format!("{node_key}#{direction}:{kind}:{index}")
}
