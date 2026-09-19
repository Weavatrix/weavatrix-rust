use super::schema_rules::field;
use super::view::owned_domains;
use crate::engine::RepositoryState;
use blazingly_json::{Value, json};
use weavatrix_graph::Node;

pub(super) fn selected(
    state: &RepositoryState,
    before: &[&Node],
    after: &[&Node],
    max: usize,
) -> Vec<Value> {
    let mut keys = before
        .iter()
        .chain(after.iter())
        .map(|node| {
            let domains = owned_domains(state, node.id.as_str());
            (
                node.label.clone(),
                field(&domains, "package:"),
                field(&domains, "catalog:"),
                field(&domains, "exposure:"),
            )
        })
        .collect::<Vec<_>>();
    keys.sort();
    keys.dedup();
    state
        .graph()
        .nodes()
        .iter()
        .filter(|node| node.kind.as_str() == "agent.skill")
        .filter(|node| {
            let domains = owned_domains(state, node.id.as_str());
            keys.iter().any(|(name, package, catalog, exposure)| {
                declared_tool(&domains, name)
                    && same_identity(
                        &domains,
                        package.as_deref(),
                        catalog.as_deref(),
                        exposure.as_deref(),
                    )
            })
        })
        .take(max)
        .map(|node| {
            json!({
                "id": node.id,
                "label": node.label,
                "kind": node.kind,
                "span": node.span,
                "relation": "declared",
                "usage": "allowed-tools declaration, not a proven call"
            })
        })
        .collect()
}

fn declared_tool(domains: &[Value], name: &str) -> bool {
    let expected = format!("declared_allowed_tools:{name}");
    domains
        .iter()
        .any(|item| item["name"].as_str() == Some(expected.as_str()))
}

fn same_identity(
    domains: &[Value],
    package: Option<&str>,
    catalog: Option<&str>,
    exposure: Option<&str>,
) -> bool {
    matches!(
        (field(domains, "package:").as_deref(), package),
        (Some(skill), Some(tool))
            if skill == tool
                || skill.starts_with(&format!("{tool}/"))
                || tool.starts_with(&format!("{skill}/"))
    ) && catalog.is_none_or(|expected| {
        field(domains, "catalog:")
            .as_deref()
            .is_none_or(|actual| actual == expected)
    }) && exposure.is_none_or(|expected| {
        field(domains, "exposure:")
            .as_deref()
            .is_none_or(|actual| actual == expected)
    })
}
