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

#[cfg(test)]
mod tests {
    use super::{
        abi_kind, artifact_kind, binds, constructor_kind, consumer_kind, error_kind, event_kind,
        function_kind, member_kind,
    };

    #[test]
    fn web3_kinds_are_custom_or_fall_back() {
        assert_eq!(function_kind().as_str(), "web3.abi.function");
        assert_eq!(event_kind().as_str(), "web3.abi.event");
        assert_eq!(error_kind().as_str(), "web3.abi.error");
        assert_eq!(constructor_kind().as_str(), "web3.abi.constructor");
        assert_eq!(artifact_kind().as_str(), "web3.artifact");
        assert_eq!(consumer_kind().as_str(), "web3.consumer");
        assert_eq!(abi_kind().as_str(), "web3.abi");
        assert_eq!(binds().as_str(), "web3.binds");
        assert_eq!(member_kind("function").as_str(), "web3.abi.function");
        assert_eq!(member_kind("event").as_str(), "web3.abi.event");
        assert_eq!(member_kind("error").as_str(), "web3.abi.error");
        assert_eq!(member_kind("fallback").as_str(), "web3.abi.constructor");
        assert_eq!(member_kind("other").as_str(), "web3.abi");
    }
}
