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
        changes.push(diff_tool(
            state,
            name,
            named(&before_tools, name),
            named(&after_tools, name),
            &transforms,
            before_complete,
            after_complete,
        ));
    }
    let found = changes.len();
    let shown = changes.into_iter().take(max).collect::<Vec<_>>();
    Ok(json!({
        "changes": shown,
        "consumers": super::consumers::selected(state, &before_tools, &after_tools, max),
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

fn named<'a>(tools: &'a [&Node], name: &str) -> ToolMatch<'a> {
    let hits = tools
        .iter()
        .copied()
        .filter(|node| node.label == name)
        .collect::<Vec<_>>();
    match hits.as_slice() {
        [] => ToolMatch::None,
        [one] => ToolMatch::One(one),
        many => ToolMatch::Ambiguous(many.len()),
    }
}

enum ToolMatch<'a> {
    None,
    One(&'a Node),
    Ambiguous(usize),
}

fn diff_tool(
    state: &RepositoryState,
    name: &str,
    before: ToolMatch<'_>,
    after: ToolMatch<'_>,
    transforms: &[&Node],
    before_complete: bool,
    after_complete: bool,
) -> Value {
    if let (count, true) = match (&before, &after) {
        (ToolMatch::Ambiguous(left), ToolMatch::Ambiguous(right)) => (*left.max(right), true),
        (ToolMatch::Ambiguous(left), _) => (*left, true),
        (_, ToolMatch::Ambiguous(right)) => (*right, true),
        _ => (0, false),
    } {
        return json!({
            "tool": name,
            "change": "ambiguous-identity",
            "compatibility": "undetermined",
            "matches": count,
            "premises": ["multiple tools share this label; pick an exact server or catalog"]
        });
    }
    match (before, after) {
        (ToolMatch::None, ToolMatch::One(after)) => json!({
            "tool": name,
            "change": "added",
            "compatibility": "undetermined",
            "witness": after.span
        }),
        (ToolMatch::One(before), ToolMatch::None) => {
            let removed = before_complete && after_complete;
            json!({
                "tool": name,
                "change": if removed { "removed" } else { "unconfirmed" },
                "compatibility": if removed { "proven-incompatible" } else { "undetermined" },
                "witness": before.span
            })
        }
        (ToolMatch::One(before), ToolMatch::One(after)) => {
            schema_cmp::schema_change(state, name, before, after, transforms)
        }
        _ => json!({"tool": name, "change": "none", "compatibility": "undetermined"}),
    }
}
