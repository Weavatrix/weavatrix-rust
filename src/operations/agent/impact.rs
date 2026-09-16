use super::view::{self, owned_domains};
use crate::engine::RepositoryState;
use crate::operations::{arg_str, optional_u64, reject_unknown_arguments};
use blazingly_json::{Value, json};
use weavatrix_graph::Node;

pub(super) fn change_impact(state: &RepositoryState, args: &Value) -> Result<Value, String> {
    reject_unknown_arguments(
        "agent_change_impact",
        args,
        &["before", "after", "max_results"],
    )?;
    let before = arg_str(args, "before")?;
    let after = arg_str(args, "after")?;
    let max = usize::try_from(optional_u64(args, "max_results")?.unwrap_or(100))
        .map_err(|_| "max_results is too large")?;
    if max == 0 || max > 500 {
        return Err("max_results must be between 1 and 500".to_owned());
    }
    let before_tools = tools_in(state, before);
    let after_tools = tools_in(state, after);
    let transforms = view::nodes(state, "agent.transform", None);
    let mut names = before_tools
        .iter()
        .chain(after_tools.iter())
        .map(|node| node.label.clone())
        .collect::<Vec<_>>();
    names.sort_unstable();
    names.dedup();
    let before_complete = catalog_complete(state, before);
    let mut changes = Vec::new();
    for name in names {
        let left = before_tools.iter().find(|node| node.label == name);
        let right = after_tools.iter().find(|node| node.label == name);
        changes.push(diff_tool(
            state,
            &name,
            left,
            right,
            &transforms,
            before_complete,
        ));
    }
    let found = changes.len();
    let shown = changes.into_iter().take(max).collect::<Vec<_>>();
    Ok(json!({
        "changes": shown,
        "consumers": consumers(state, &before_tools, &after_tools, max),
        "bounds": {
            "truncated": found > max,
            "found": found,
            "shown": found.min(max),
            "reasons": if found > max { vec!["max_results"] } else { Vec::<&str>::new() },
            "revision": state.snapshot().revision,
            "runtime": false,
            "authorization": "none"
        }
    }))
}

fn tools_in<'a>(state: &'a RepositoryState, path: &str) -> Vec<&'a Node> {
    view::nodes(state, "agent.tool", Some(path))
}

fn catalog_complete(state: &RepositoryState, path: &str) -> bool {
    view::nodes(state, "agent.catalog", Some(path))
        .iter()
        .all(|node| {
            !owned_domains(state, node.id.as_str()).iter().any(|item| {
                item["name"].as_str() == Some("pagination:incomplete")
                    || item["name"].as_str() == Some("completeness:partial")
            })
        })
}

fn diff_tool(
    state: &RepositoryState,
    name: &str,
    before: Option<&&Node>,
    after: Option<&&Node>,
    transforms: &[&Node],
    before_complete: bool,
) -> Value {
    match (before, after) {
        (None, Some(after)) => json!({
            "tool": name,
            "change": "added",
            "compatibility": "undetermined",
            "witness": after.span
        }),
        (Some(before), None) => json!({
            "tool": name,
            "change": if before_complete { "removed" } else { "unconfirmed" },
            "compatibility": if before_complete { "proven-incompatible" } else { "undetermined" },
            "witness": before.span
        }),
        (Some(before), Some(after)) => schema_change(state, name, before, after, transforms),
        (None, None) => json!({"tool": name, "change": "none", "compatibility": "undetermined"}),
    }
}

fn schema_change(
    state: &RepositoryState,
    name: &str,
    before: &Node,
    after: &Node,
    transforms: &[&Node],
) -> Value {
    let left = owned_domains(state, before.id.as_str());
    let right = owned_domains(state, after.id.as_str());
    let before_req = field(&left, "required:").unwrap_or_default();
    let after_req = field(&right, "required:").unwrap_or_default();
    let before_desc = field(&left, "description:");
    let after_desc = field(&right, "description:");
    let unresolved = right
        .iter()
        .any(|item| item["name"] == "schema:unresolved_ref");
    let added_required = added_csv(&before_req, &after_req);
    let transform = transform_named(state, name, transforms, "exposure:");
    let linked = transform.or_else(|| transform_named(state, name, transforms, "upstream:"));
    let filled = transform
        .as_ref()
        .map(|item| injected(state, item))
        .unwrap_or_default();
    let remaining = added_required
        .iter()
        .filter(|item| !filled.iter().any(|inject| inject == *item))
        .cloned()
        .collect::<Vec<_>>();
    let compatibility = if unresolved {
        "undetermined"
    } else if remaining.is_empty() && added_required.is_empty() {
        if before_desc == after_desc {
            "proven-compatible"
        } else {
            "description-only"
        }
    } else if remaining.is_empty() {
        "proven-compatible"
    } else {
        "proven-incompatible"
    };
    json!({
        "tool": name,
        "change": "schema",
        "compatibility": compatibility,
        "added_required": added_required,
        "adapter_fills": filled,
        "breaking_for_exposure": remaining,
        "description_changed": before_desc != after_desc,
        "transform": linked.map(|node| node.label.clone()),
        "witness": after.span
    })
}

fn transform_named<'a>(
    state: &'a RepositoryState,
    name: &str,
    transforms: &'a [&Node],
    prefix: &str,
) -> Option<&'a Node> {
    let expected = format!("{prefix}{name}");
    transforms.iter().copied().find(|node| {
        owned_domains(state, node.id.as_str())
            .iter()
            .any(|item| item["name"].as_str() == Some(expected.as_str()))
    })
}

fn injected(state: &RepositoryState, transform: &Node) -> Vec<String> {
    owned_domains(state, transform.id.as_str())
        .iter()
        .filter_map(|item| {
            item["name"]
                .as_str()
                .and_then(|name| name.strip_prefix("inject:"))
                .map(ToOwned::to_owned)
        })
        .collect()
}

fn consumers(state: &RepositoryState, before: &[&Node], after: &[&Node], max: usize) -> Vec<Value> {
    let mut names = before
        .iter()
        .chain(after.iter())
        .map(|node| node.label.as_str())
        .collect::<Vec<_>>();
    names.sort_unstable();
    names.dedup();
    state
        .graph()
        .nodes()
        .iter()
        .filter(|node| node.kind.as_str() == "agent.skill")
        .filter_map(|node| {
            let domains = owned_domains(state, node.id.as_str());
            let hit = names.iter().any(|name| {
                domains.iter().any(|item| {
                    item["name"]
                        .as_str()
                        .is_some_and(|label| label.contains(name))
                })
            });
            hit.then(|| view::selected(node))
        })
        .take(max)
        .collect()
}

fn field(domains: &[Value], prefix: &str) -> Option<String> {
    domains.iter().find_map(|item| {
        item["name"]
            .as_str()
            .and_then(|name| name.strip_prefix(prefix))
            .map(ToOwned::to_owned)
    })
}

fn added_csv(before: &str, after: &str) -> Vec<String> {
    after
        .split(',')
        .filter(|item| !item.is_empty() && !before.split(',').any(|old| old == *item))
        .map(ToOwned::to_owned)
        .collect()
}
