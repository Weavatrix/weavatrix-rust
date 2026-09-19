use super::model::NodeSemantics;

const SUPPORTED: &[(&str, &[&str])] = &[
    ("n8n-nodes-base.manualTrigger", &["1"]),
    ("n8n-nodes-base.webhook", &["1", "2"]),
    ("n8n-nodes-base.scheduleTrigger", &["1", "1.1", "1.2"]),
    ("n8n-nodes-base.executeWorkflowTrigger", &["1"]),
    ("n8n-nodes-base.errorTrigger", &["1"]),
    (
        "n8n-nodes-base.httpRequest",
        &["1", "2", "3", "4", "4.1", "4.2"],
    ),
    ("n8n-nodes-base.respondToWebhook", &["1", "1.1"]),
    ("n8n-nodes-base.if", &["1", "2", "2.2"]),
    ("n8n-nodes-base.switch", &["1", "3", "3.2"]),
    ("n8n-nodes-base.set", &["1", "3", "3.4"]),
    ("n8n-nodes-base.code", &["1", "2"]),
    ("n8n-nodes-base.function", &["1"]),
    ("n8n-nodes-base.executeWorkflow", &["1"]),
    ("n8n-nodes-base.merge", &["1", "2", "3", "3.1"]),
    ("n8n-nodes-base.splitInBatches", &["1", "3"]),
    ("n8n-nodes-base.wait", &["1", "1.1"]),
    ("n8n-nodes-base.noOp", &["1"]),
    ("@n8n/n8n-nodes-langchain.agent", &["1", "1.7", "2"]),
    (
        "@n8n/n8n-nodes-langchain.lmChatOpenAi",
        &["1", "1.2", "1.7"],
    ),
    ("@n8n/n8n-nodes-langchain.memoryBufferWindow", &["1", "1.3"]),
    ("@n8n/n8n-nodes-langchain.toolCalculator", &["1"]),
    ("@n8n/n8n-nodes-langchain.toolCode", &["1", "1.1"]),
    ("@n8n/n8n-nodes-langchain.toolHttpRequest", &["1", "1.1"]),
    ("@n8n/n8n-nodes-langchain.chatTrigger", &["1", "1.1"]),
    ("@n8n/n8n-nodes-langchain.embeddingsOpenAi", &["1", "1.2"]),
];

#[must_use]
pub(super) fn semantics(type_name: &str, type_version: &str) -> NodeSemantics {
    if type_name.starts_with("@n8n/n8n-nodes-langchain.") {
        return if known_version(type_name, type_version)
            || type_version == "1"
            || type_version == "1.7"
        {
            NodeSemantics::Supported
        } else {
            NodeSemantics::Unsupported
        };
    }
    match SUPPORTED.iter().find(|(name, _)| *name == type_name) {
        Some((_, versions)) if versions.contains(&type_version) => NodeSemantics::Supported,
        Some(_) => NodeSemantics::Unsupported,
        None => NodeSemantics::StructureOnly,
    }
}

fn known_version(type_name: &str, type_version: &str) -> bool {
    SUPPORTED
        .iter()
        .find(|(name, _)| *name == type_name)
        .is_some_and(|(_, versions)| versions.contains(&type_version))
}

#[cfg(test)]
mod tests {
    use super::super::model::NodeSemantics;
    use super::semantics;

    #[test]
    fn node_families_name_supported_unsupported_and_structure_only() {
        assert_eq!(
            semantics("n8n-nodes-base.httpRequest", "4.2"),
            NodeSemantics::Supported
        );
        assert_eq!(
            semantics("n8n-nodes-base.httpRequest", "9"),
            NodeSemantics::Unsupported
        );
        assert_eq!(
            semantics("custom.unknown", "1"),
            NodeSemantics::StructureOnly
        );
        assert_eq!(
            semantics("@n8n/n8n-nodes-langchain.agent", "2"),
            NodeSemantics::Supported
        );
        assert_eq!(
            semantics("@n8n/n8n-nodes-langchain.unknown", "1"),
            NodeSemantics::Supported
        );
        assert_eq!(
            semantics("@n8n/n8n-nodes-langchain.unknown", "9"),
            NodeSemantics::Unsupported
        );
    }
}
