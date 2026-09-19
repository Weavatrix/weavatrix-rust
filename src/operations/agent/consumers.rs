use super::schema_cmp::field;
use super::view::{self, owned_domains};
use crate::engine::RepositoryState;
use blazingly_json::Value;
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
            (node.label.clone(), field(&domains, "package:"))
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
            keys.iter().any(|(name, package)| {
                declared_tool(&domains, name) && same_root(&domains, package.as_deref())
            })
        })
        .take(max)
        .map(view::selected)
        .collect()
}

fn declared_tool(domains: &[Value], name: &str) -> bool {
    let expected = format!("declared_allowed_tools:{name}");
    domains
        .iter()
        .any(|item| item["name"].as_str() == Some(expected.as_str()))
}

fn same_root(domains: &[Value], package: Option<&str>) -> bool {
    match (field(domains, "package:").as_deref(), package) {
        (Some(skill), Some(tool)) => {
            skill == tool
                || tool.starts_with(&format!("{skill}/"))
                || skill.starts_with(&format!("{tool}/"))
        }
        (None, Some(_)) => false,
        _ => true,
    }
}
