use crate::model::Diagnostic;
use weavatrix_graph::{EdgeKind, NodeKind, SourceSpan};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Profile {
    Portable,
    Native,
    Cursor,
    Claude,
    Codex,
    Grok,
    Kiro,
}

impl Profile {
    pub(super) const fn as_str(self) -> &'static str {
        match self {
            Self::Portable => "portable",
            Self::Native => "native",
            Self::Cursor => "cursor",
            Self::Claude => "claude",
            Self::Codex => "codex",
            Self::Grok => "grok",
            Self::Kiro => "kiro",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Completeness {
    Valid,
    Partial,
    Invalid,
}

impl Completeness {
    pub(super) const fn as_str(self) -> &'static str {
        match self {
            Self::Valid => "valid",
            Self::Partial => "partial",
            Self::Invalid => "invalid",
        }
    }
}

#[derive(Debug, Clone)]
pub(super) struct PluginRecord {
    pub name: String,
    pub profile: Profile,
    pub completeness: Completeness,
    pub schema: Option<String>,
    pub version: Option<String>,
    pub package_root: String,
    pub unknown_fields: Vec<String>,
    pub extensions: Vec<String>,
    pub declared_paths: Vec<String>,
    pub diagnostics: Vec<Diagnostic>,
    pub span: SourceSpan,
}

#[derive(Debug, Clone)]
pub(super) struct McpConfig {
    pub profile: Profile,
    pub completeness: Completeness,
    pub schema: Option<String>,
    pub package_root: String,
    pub unknown_fields: Vec<String>,
    pub servers: Vec<ServerRecord>,
    pub diagnostics: Vec<Diagnostic>,
    pub span: SourceSpan,
}

#[derive(Debug, Clone)]
pub(super) struct ServerRecord {
    pub key: String,
    pub transport: String,
    pub command: Option<String>,
    pub url: Option<String>,
    pub completeness: Completeness,
    pub span: SourceSpan,
}

#[derive(Debug, Clone)]
pub(super) struct SkillRecord {
    pub name: String,
    pub directory: String,
    pub package_root: String,
    pub completeness: Completeness,
    pub allowed_tools: Vec<String>,
    pub references: Vec<String>,
    pub diagnostics: Vec<Diagnostic>,
    pub span: SourceSpan,
}

#[must_use]
pub(super) fn plugin_kind() -> NodeKind {
    NodeKind::custom("agent.plugin").unwrap_or(NodeKind::Module)
}

#[must_use]
pub(super) fn skill_kind() -> NodeKind {
    NodeKind::custom("agent.skill").unwrap_or(NodeKind::Module)
}

#[must_use]
pub(super) fn mcp_config_kind() -> NodeKind {
    NodeKind::custom("agent.mcp_config").unwrap_or(NodeKind::Module)
}

#[must_use]
pub(super) fn mcp_server_kind() -> NodeKind {
    NodeKind::custom("agent.mcp_server").unwrap_or(NodeKind::Function)
}

#[must_use]
pub(super) fn extension_kind() -> NodeKind {
    NodeKind::custom("agent.extension").unwrap_or(NodeKind::Binding)
}

#[must_use]
pub(super) fn catalog_kind() -> NodeKind {
    NodeKind::custom("agent.catalog").unwrap_or(NodeKind::Module)
}

#[must_use]
pub(super) fn tool_kind() -> NodeKind {
    NodeKind::custom("agent.tool").unwrap_or(NodeKind::Function)
}

#[must_use]
pub(super) fn transform_kind() -> NodeKind {
    NodeKind::custom("agent.transform").unwrap_or(NodeKind::Binding)
}

#[must_use]
pub(super) fn registration_kind() -> NodeKind {
    NodeKind::custom("agent.registration").unwrap_or(NodeKind::Function)
}

#[must_use]
pub(super) fn observation_kind() -> NodeKind {
    NodeKind::custom("agent.observation").unwrap_or(NodeKind::Unknown)
}

#[must_use]
pub(super) fn a2a_kind() -> NodeKind {
    NodeKind::custom("agent.a2a_card").unwrap_or(NodeKind::Module)
}

#[must_use]
pub(super) fn declares() -> EdgeKind {
    EdgeKind::custom("declares").unwrap_or(EdgeKind::Configures)
}

#[must_use]
pub(super) fn transforms() -> EdgeKind {
    EdgeKind::custom("transforms").unwrap_or(EdgeKind::Binds)
}
