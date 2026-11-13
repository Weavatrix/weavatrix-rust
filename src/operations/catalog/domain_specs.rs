use super::definitions::ToolSpec;

pub(super) const DOMAIN_SPECS: &[ToolSpec] = &[
    ToolSpec {
        name: "n8n_inventory",
        description: super::prose::domain::N8N_INVENTORY,
        required: &[],
    },
    ToolSpec {
        name: "n8n_trace",
        description: super::prose::domain::N8N_TRACE,
        required: &["label"],
    },
    ToolSpec {
        name: "n8n_context",
        description: super::prose::domain::N8N_CONTEXT,
        required: &["label"],
    },
    ToolSpec {
        name: "dify_inventory",
        description: super::prose::domain::DIFY_INVENTORY,
        required: &[],
    },
    ToolSpec {
        name: "dify_trace",
        description: super::prose::domain::DIFY_TRACE,
        required: &["label"],
    },
    ToolSpec {
        name: "dify_context",
        description: super::prose::domain::DIFY_CONTEXT,
        required: &["label"],
    },
    ToolSpec {
        name: "agent_inventory",
        description: super::prose::domain::AGENT_INVENTORY,
        required: &[],
    },
    ToolSpec {
        name: "agent_trace",
        description: super::prose::domain::AGENT_TRACE,
        required: &["label"],
    },
    ToolSpec {
        name: "agent_context",
        description: super::prose::domain::AGENT_CONTEXT,
        required: &["label"],
    },
    ToolSpec {
        name: "agent_change_impact",
        description: super::prose::domain::AGENT_CHANGE_IMPACT,
        required: &["before", "after"],
    },
    ToolSpec {
        name: "diagram_inventory",
        description: super::prose::domain::DIAGRAM_INVENTORY,
        required: &[],
    },
    ToolSpec {
        name: "diagram_trace",
        description: super::prose::domain::DIAGRAM_TRACE,
        required: &["label"],
    },
    ToolSpec {
        name: "diagram_context",
        description: super::prose::domain::DIAGRAM_CONTEXT,
        required: &["label"],
    },
    ToolSpec {
        name: "web3_inventory",
        description: super::prose::domain::WEB3_INVENTORY,
        required: &[],
    },
    ToolSpec {
        name: "web3_trace",
        description: super::prose::domain::WEB3_TRACE,
        required: &["label"],
    },
    ToolSpec {
        name: "web3_impact",
        description: super::prose::domain::WEB3_IMPACT,
        required: &["baseline", "candidate"],
    },
    ToolSpec {
        name: "web3_context",
        description: super::prose::domain::WEB3_CONTEXT,
        required: &["label"],
    },
];
