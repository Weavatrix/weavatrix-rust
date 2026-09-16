use super::definitions::ToolSpec;

pub(super) const DOMAIN_SPECS: &[ToolSpec] = &[
    ToolSpec {
        name: "n8n_inventory",
        description: "List n8n workflows, nodes, entry points, and analysis bounds from exported JSON.",
        required: &[],
    },
    ToolSpec {
        name: "n8n_trace",
        description: "Upstream and downstream n8n port flow, output dependencies, and static subworkflow links.",
        required: &["label"],
    },
    ToolSpec {
        name: "n8n_context",
        description: "Bounded n8n context for one node: proven dependencies, expression sites, and explicit unknowns.",
        required: &["label"],
    },
    ToolSpec {
        name: "dify_inventory",
        description: "List Dify apps, nodes, modes, and analysis bounds from exported YAML.",
        required: &[],
    },
    ToolSpec {
        name: "dify_trace",
        description: "Upstream and downstream Dify port flow, selectors, and typed data relations.",
        required: &["label"],
    },
    ToolSpec {
        name: "dify_context",
        description: "Bounded Dify context for one node: proven consumers, selector sites, and explicit gaps.",
        required: &["label"],
    },
    ToolSpec {
        name: "agent_inventory",
        description: "List Agent Plugins, Skills, and MCP server bindings from local package files without launching them.",
        required: &[],
    },
    ToolSpec {
        name: "agent_trace",
        description: "Show the declared origin, profile, transforms, and package bindings for one plugin, skill, or MCP server.",
        required: &["label"],
    },
    ToolSpec {
        name: "agent_context",
        description: "Bounded agent-package context: declared bindings, source fragments, and explicit authorization gaps.",
        required: &["label"],
    },
    ToolSpec {
        name: "agent_change_impact",
        description: "Compare two supplied MCP catalog snapshots and report contract changes, adapter compensation, and declared consumers.",
        required: &["before", "after"],
    },
    ToolSpec {
        name: "diagram_inventory",
        description: "List Mermaid flowchart diagrams, native elements, and explicit sidecar bindings.",
        required: &[],
    },
    ToolSpec {
        name: "diagram_trace",
        description: "Walk declared_architecture arrows inside one Mermaid diagram without treating them as production Calls.",
        required: &["label"],
    },
    ToolSpec {
        name: "diagram_context",
        description: "Bounded Mermaid context: source fragments, explicit bindings, and gaps that remain unproven.",
        required: &["label"],
    },
];
