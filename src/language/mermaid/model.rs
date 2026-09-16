use crate::model::Diagnostic;
use weavatrix_graph::{EdgeKind, NodeKind, SourceSpan};

pub(super) const MAX_NODES: usize = 2_000;
pub(super) const MAX_EDGES: usize = 4_000;

#[derive(Debug, Clone)]
pub(super) struct Region {
    pub index: usize,
    pub start: usize,
    pub end: usize,
}

#[derive(Debug, Clone)]
pub(super) struct Diagram {
    pub key: String,
    pub direction: String,
    pub kind: String,
    pub completeness: Completeness,
    pub span: SourceSpan,
    pub elements: Vec<Element>,
    pub groups: Vec<Group>,
    pub relations: Vec<Relation>,
    pub diagnostics: Vec<Diagnostic>,
}

#[derive(Debug, Clone)]
pub(super) struct Element {
    pub id: String,
    pub label: String,
    pub span: SourceSpan,
    pub group: Option<String>,
    pub occurrences: Vec<SourceSpan>,
}

#[derive(Debug, Clone)]
pub(super) struct Group {
    pub id: String,
    pub title: String,
    pub span: SourceSpan,
}

#[derive(Debug, Clone)]
pub(super) struct Relation {
    pub from: String,
    pub to: String,
    pub label: String,
    pub marker: String,
    pub occurrence: u32,
    pub span: SourceSpan,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Completeness {
    Full,
    Partial,
}

impl Completeness {
    pub(super) const fn as_str(self) -> &'static str {
        match self {
            Self::Full => "full",
            Self::Partial => "partial",
        }
    }
}

#[must_use]
pub(super) fn diagram_kind() -> NodeKind {
    NodeKind::custom("mermaid.diagram").unwrap_or(NodeKind::Module)
}

#[must_use]
pub(super) fn element_kind() -> NodeKind {
    NodeKind::custom("mermaid.element").unwrap_or(NodeKind::Function)
}

#[must_use]
pub(super) fn group_kind() -> NodeKind {
    NodeKind::custom("mermaid.group").unwrap_or(NodeKind::Module)
}

#[must_use]
pub(super) fn binding_kind() -> NodeKind {
    NodeKind::custom("mermaid.binding").unwrap_or(NodeKind::Binding)
}

#[must_use]
pub(super) fn declared_architecture() -> EdgeKind {
    EdgeKind::custom("declared_architecture").unwrap_or(EdgeKind::Configures)
}
