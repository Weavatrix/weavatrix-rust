mod consumers;
mod impact;
mod schema_cmp;
mod schema_rules;
mod view;

use crate::engine::RepositoryState;
use crate::operations::{arg_str, optional_str, optional_u64, reject_unknown_arguments};
use blazingly_json::{Value, json};
use weavatrix_graph::Node;

pub(super) fn inventory(state: &RepositoryState, args: &Value) -> Result<Value, String> {
    reject_unknown_arguments("agent_inventory", args, &["path", "max_results"])?;
    let path = optional_str(args, "path")?;
    let max = usize::try_from(optional_u64(args, "max_results")?.unwrap_or(200))
        .map_err(|_| "max_results is too large")?;
    if max == 0 || max > 500 {
        return Err("max_results must be between 1 and 500".to_owned());
    }
    let plugins = view::nodes(state, "agent.plugin", path);
    let skills = view::nodes(state, "agent.skill", path);
    let servers = view::nodes(state, "agent.mcp_server", path);
    let catalogs = view::nodes(state, "agent.catalog", path);
    let tools = view::nodes(state, "agent.tool", path);
    let observations = view::nodes(state, "agent.observation", path);
    let found = plugins.len() + skills.len() + servers.len() + catalogs.len() + tools.len();
    let shown_plugins = take(&plugins, max);
    let shown_skills = take(&skills, max);
    let shown_servers = take(&servers, max);
    let shown_catalogs = take(&catalogs, max);
    let shown_tools = take(&tools, max);
    let shown_obs = take(&observations, max);
    let shown = shown_plugins.len()
        + shown_skills.len()
        + shown_servers.len()
        + shown_catalogs.len()
        + shown_tools.len();
    let page_cut = found > shown;
    Ok(json!({
        "plugins": shown_plugins.iter().map(|node| view::row(state, node)).collect::<Vec<_>>(),
        "skills": shown_skills.iter().map(|node| view::row(state, node)).collect::<Vec<_>>(),
        "mcp_servers": shown_servers.iter().map(|node| view::row(state, node)).collect::<Vec<_>>(),
        "catalogs": shown_catalogs.iter().map(|node| view::row(state, node)).collect::<Vec<_>>(),
        "tools": shown_tools.iter().map(|node| view::row(state, node)).collect::<Vec<_>>(),
        "observations": shown_obs.iter().map(|node| view::row(state, node)).collect::<Vec<_>>(),
        "coverage": view::coverage(state, path),
        "bounds": bounds(state, found, shown, page_cut)
    }))
}

pub(super) fn trace(state: &RepositoryState, args: &Value) -> Result<Value, String> {
    reject_unknown_arguments("agent_trace", args, &["label", "max_related"])?;
    let label = arg_str(args, "label")?;
    let max_related = usize::try_from(optional_u64(args, "max_related")?.unwrap_or(48))
        .map_err(|_| "max_related is too large")?;
    if max_related == 0 || max_related > 200 {
        return Err("max_related must be between 1 and 200".to_owned());
    }
    let index = state.resolve_node(label)?;
    let node = state.node(index)?;
    let relations = view::owned_domains(state, node.id.as_str());
    let found = relations.len();
    let cut = found > max_related;
    Ok(json!({
        "selected": view::selected(node),
        "relations": relations.into_iter().take(max_related).collect::<Vec<_>>(),
        "children": view::children(state, node.id.as_str()),
        "coverage": view::coverage(state, node.span.as_ref().map(|span| span.file.as_str())),
        "bounds": bounds(state, found, found.min(max_related), cut)
    }))
}

pub(super) fn change_impact(state: &RepositoryState, args: &Value) -> Result<Value, String> {
    impact::change_impact(state, args)
}

pub(super) fn context(state: &RepositoryState, args: &Value) -> Result<Value, String> {
    reject_unknown_arguments("agent_context", args, &["label", "task", "max_related"])?;
    let label = arg_str(args, "label")?;
    let task = optional_str(args, "task")?.unwrap_or("inspect");
    let max_related = usize::try_from(optional_u64(args, "max_related")?.unwrap_or(24))
        .map_err(|_| "max_related is too large")?;
    if max_related == 0 || max_related > 200 {
        return Err("max_related must be between 1 and 200".to_owned());
    }
    let index = state.resolve_node(label)?;
    let node = state.node(index)?;
    let domains = view::owned_domains(state, node.id.as_str());
    let granted = domains.iter().any(|item| {
        item["name"]
            .as_str()
            .is_some_and(|name| name.starts_with("declared_allowed_tools:"))
    });
    let mut gaps = vec![
        "runtime catalog is not provided".to_owned(),
        "commands and hooks were not executed".to_owned(),
    ];
    if granted {
        gaps.push("allowed-tools is declared only; no grant was inferred".to_owned());
    }
    Ok(json!({
        "selected": view::selected(node),
        "task": task,
        "relations": domains.into_iter().take(max_related).collect::<Vec<_>>(),
        "fragments": view::fragments(state, node),
        "gaps": gaps,
        "coverage": view::coverage(state, node.span.as_ref().map(|span| span.file.as_str())),
        "bounds": {
            "truncated": false,
            "found": 1,
            "shown": 1,
            "reasons": Vec::<&str>::new(),
            "revision": state.snapshot().revision,
            "complete": false,
            "runtime": false,
            "executed": false,
            "authorization": "none"
        }
    }))
}

fn take<'a>(nodes: &'a [&Node], max: usize) -> Vec<&'a Node> {
    nodes.iter().copied().take(max).collect()
}

fn bounds(state: &RepositoryState, found: usize, shown: usize, truncated: bool) -> Value {
    json!({
        "truncated": truncated,
        "found": found,
        "shown": shown,
        "reasons": if truncated { vec!["max_results"] } else { Vec::<&str>::new() },
        "revision": state.snapshot().revision,
        "runtime": false,
        "executed": false,
        "authorization": "none"
    })
}
