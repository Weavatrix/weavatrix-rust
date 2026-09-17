use weavatrix_graph::{EdgeKind, NodeKind};

#[must_use]
pub(super) fn abi_kind() -> NodeKind {
    NodeKind::custom("web3.abi").unwrap_or(NodeKind::Module)
}

#[must_use]
pub(super) fn function_kind() -> NodeKind {
    NodeKind::custom("web3.abi.function").unwrap_or(NodeKind::Function)
}

#[must_use]
pub(super) fn event_kind() -> NodeKind {
    NodeKind::custom("web3.abi.event").unwrap_or(NodeKind::Function)
}

#[must_use]
pub(super) fn error_kind() -> NodeKind {
    NodeKind::custom("web3.abi.error").unwrap_or(NodeKind::TypeAlias)
}

#[must_use]
pub(super) fn constructor_kind() -> NodeKind {
    NodeKind::custom("web3.abi.constructor").unwrap_or(NodeKind::Function)
}

#[must_use]
pub(super) fn artifact_kind() -> NodeKind {
    NodeKind::custom("web3.artifact").unwrap_or(NodeKind::Module)
}

#[must_use]
pub(super) fn consumer_kind() -> NodeKind {
    NodeKind::custom("web3.consumer").unwrap_or(NodeKind::Function)
}

#[must_use]
pub(crate) fn binds() -> EdgeKind {
    EdgeKind::custom("web3.binds").unwrap_or(EdgeKind::Reads)
}

#[must_use]
pub(super) fn member_kind(kind: &str) -> NodeKind {
    match kind {
        "function" => function_kind(),
        "event" => event_kind(),
        "error" => error_kind(),
        "constructor" | "fallback" | "receive" => constructor_kind(),
        _ => abi_kind(),
    }
}
