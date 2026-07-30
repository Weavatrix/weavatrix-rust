use std::str::FromStr;

/// Selects a bounded operation catalog independently of any transport.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum ToolProfile {
    /// Code intelligence plus semantic, SEO, and memory extensions.
    #[default]
    All,
    /// Repository and coding-agent intelligence without SEO-specific tools.
    Code,
    /// Content-graph, search, semantic, and SEO analysis.
    Seo,
}

impl ToolProfile {
    #[must_use]
    pub fn allows(self, tool: &str) -> bool {
        match self {
            Self::All => true,
            Self::Code => tool != "seo_link_suggestions",
            Self::Seo => matches!(
                tool,
                "graph_stats"
                    | "get_node"
                    | "get_neighbors"
                    | "query_graph"
                    | "shortest_path"
                    | "search_code"
                    | "read_source"
                    | "context_bundle"
                    | "list_communities"
                    | "get_community"
                    | "module_map"
                    | "rebuild_graph"
                    | "open_repo"
                    | "list_known_repos"
                    | "semantic_link"
                    | "vector_search"
                    | "seo_link_suggestions"
                    | "memory_context"
            ),
        }
    }
}

impl FromStr for ToolProfile {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "all" => Ok(Self::All),
            "code" => Ok(Self::Code),
            "seo" | "content" => Ok(Self::Seo),
            _ => Err(format!(
                "unknown tool profile {value:?}; expected all, code, or seo"
            )),
        }
    }
}
