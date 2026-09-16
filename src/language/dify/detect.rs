use crate::language::yaml_doc::Node;

pub(crate) const DEFAULT_FILE_BYTES: u64 = 1_500_000;
pub(crate) const MAX_FILE_BYTES: u64 = 16 * 1024 * 1024;
pub(crate) const MAX_NODES: usize = 5_000;
pub(crate) const MAX_EDGES: usize = 30_000;

#[must_use]
pub(crate) fn looks_promising(raw: &str) -> bool {
    let has_kind = raw.contains("kind: app") || raw.contains("kind:app");
    let has_mode = raw.contains("mode:");
    has_kind && has_mode && !raw.contains("apiVersion:")
}

#[must_use]
pub(crate) fn is_export(root: &Node) -> bool {
    root.get("kind").and_then(Node::as_str) == Some("app") && root.get("app").is_some()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Mode {
    Workflow,
    AdvancedChat,
    Other,
}

impl Mode {
    pub(crate) fn parse(value: &str) -> Self {
        match value {
            "workflow" => Self::Workflow,
            "advanced-chat" => Self::AdvancedChat,
            _ => Self::Other,
        }
    }

    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            Self::Workflow => "workflow",
            Self::AdvancedChat => "advanced-chat",
            Self::Other => "other",
        }
    }

    pub(crate) const fn supported(self) -> bool {
        matches!(self, Self::Workflow | Self::AdvancedChat)
    }
}

#[must_use]
pub(crate) fn known_dsl(version: &str) -> bool {
    matches!(
        version,
        "0.1.0" | "0.1.1" | "0.1.2" | "0.1.3" | "0.1.4" | "0.1.5" | "0.3.0" | "0.3.1" | "0.7.0"
    )
}

#[must_use]
pub(crate) fn known_node(type_name: &str) -> bool {
    matches!(
        type_name,
        "start"
            | "end"
            | "llm"
            | "code"
            | "template-transform"
            | "iteration"
            | "iteration-start"
            | "loop"
            | "if-else"
            | "variable-assigner"
            | "assigner"
            | "answer"
            | "knowledge-retrieval"
            | "tool"
            | "http-request"
            | "question-classifier"
            | "parameter-extractor"
    )
}
