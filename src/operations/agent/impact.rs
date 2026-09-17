use super::schema_cmp;
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
    let before_tools = view::nodes(state, "agent.tool", Some(before));
    let after_tools = view::nodes(state, "agent.tool", Some(after));
    let transforms = view::nodes(state, "agent.transform", Some(after));
    let mut names = before_tools
        .iter()
        .chain(after_tools.iter())
        .map(|node| node.label.clone())
        .collect::<Vec<_>>();
    names.sort_unstable();
    names.dedup();
    let before_complete = catalog_complete(state, before);
    let after_complete = catalog_complete(state, after);
    let mut changes = Vec::new();
    for name in &names {
        let left = before_tools.iter().find(|node| node.label == *name);
        let right = after_tools.iter().find(|node| node.label == *name);
        changes.push(diff_tool(
            state,
            name,
            left,
            right,
            &transforms,
            before_complete,
            after_complete,
        ));
    }
    let found = changes.len();
    let shown = changes.into_iter().take(max).collect::<Vec<_>>();
    let name_refs = names.iter().map(String::as_str).collect::<Vec<_>>();
    Ok(json!({
        "changes": shown,
        "consumers": schema_cmp::selected_consumers(state, &name_refs, max),
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

fn catalog_complete(state: &RepositoryState, path: &str) -> bool {
    let catalogs = view::nodes(state, "agent.catalog", Some(path));
    !catalogs.is_empty()
        && catalogs.iter().all(|node| {
            let domains = owned_domains(state, node.id.as_str());
            !domains.iter().any(|item| {
                item["name"].as_str() == Some("pagination:incomplete")
                    || item["name"].as_str() == Some("completeness:partial")
            }) && domains
                .iter()
                .any(|item| item["name"].as_str() == Some("completeness:valid"))
        })
}

fn diff_tool(
    state: &RepositoryState,
    name: &str,
    before: Option<&&Node>,
    after: Option<&&Node>,
    transforms: &[&Node],
    before_complete: bool,
    after_complete: bool,
) -> Value {
    match (before, after) {
        (None, Some(after)) => json!({
            "tool": name,
            "change": "added",
            "compatibility": "undetermined",
            "witness": after.span
        }),
        (Some(before), None) => {
            let removed = before_complete && after_complete;
            json!({
                "tool": name,
                "change": if removed { "removed" } else { "unconfirmed" },
                "compatibility": if removed { "proven-incompatible" } else { "undetermined" },
                "witness": before.span
            })
        }
        (Some(before), Some(after)) => {
            schema_cmp::schema_change(state, name, before, after, transforms)
        }
        (None, None) => json!({"tool": name, "change": "none", "compatibility": "undetermined"}),
    }
}
