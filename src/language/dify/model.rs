use crate::model::Diagnostic;
use weavatrix_graph::{EdgeKind, NodeKind, SourceSpan};

#[derive(Debug, Clone, Default)]
pub(super) struct Coverage {
    pub structure_accepted: u32,
    pub structure_seen: u32,
    pub semantics_supported: u32,
    pub semantics_seen: u32,
    pub variables_resolved: u32,
    pub variables_seen: u32,
    pub locations_exact: u32,
    pub locations_seen: u32,
}

#[derive(Debug, Clone)]
pub(super) struct DomainBatch {
    pub apps: Vec<AppRecord>,
    pub diagnostics: Vec<Diagnostic>,
    pub truncated: bool,
}

#[derive(Debug, Clone)]
pub(super) struct AppRecord {
    pub key: String,
    pub name: String,
    pub mode: String,
    pub dsl_version: String,
    #[allow(dead_code)]
    pub supported: bool,
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
    pub title: String,
    pub type_name: String,
    pub parent: Option<String>,
    pub span: SourceSpan,
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

pub(super) fn app_kind() -> NodeKind {
    NodeKind::custom("dify.app").unwrap_or(NodeKind::Module)
}

pub(super) fn node_kind() -> NodeKind {
    NodeKind::custom("dify.node").unwrap_or(NodeKind::Function)
}

pub(super) fn port_kind() -> NodeKind {
    NodeKind::custom("dify.port").unwrap_or(NodeKind::Binding)
}

pub(super) fn flows_to() -> EdgeKind {
    EdgeKind::custom("flows_to").unwrap_or(EdgeKind::Calls)
}

pub(super) fn depends_on_output() -> EdgeKind {
    EdgeKind::custom("depends_on_output").unwrap_or(EdgeKind::DependsOn)
}

pub(super) fn binds_input() -> EdgeKind {
    EdgeKind::custom("binds_input").unwrap_or(EdgeKind::Binds)
}

pub(super) fn reads_variable() -> EdgeKind {
    EdgeKind::custom("reads_variable").unwrap_or(EdgeKind::Reads)
}

pub(super) fn writes_variable() -> EdgeKind {
    EdgeKind::custom("writes_variable").unwrap_or(EdgeKind::Writes)
}

pub(super) fn uses_model() -> EdgeKind {
    EdgeKind::custom("uses_model").unwrap_or(EdgeKind::Configures)
}

pub(super) fn invokes_tool() -> EdgeKind {
    EdgeKind::custom("invokes_tool").unwrap_or(EdgeKind::Calls)
}

pub(super) fn queries_knowledge() -> EdgeKind {
    EdgeKind::custom("queries_knowledge").unwrap_or(EdgeKind::Reads)
}

pub(super) fn imports_plugin() -> EdgeKind {
    EdgeKind::custom("imports_plugin").unwrap_or(EdgeKind::Imports)
}

pub(super) fn app_key(origin: &str) -> String {
    format!("dify::{origin}")
}

pub(super) fn node_key(app: &str, id: &str) -> String {
    format!("{app}::{id}")
}
