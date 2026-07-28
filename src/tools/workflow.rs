use crate::tools::{arg_bool, arg_str, arg_u64};
use crate::{RepositoryState, Weavatrix};
use blazingly_json::{Value, json};
use std::collections::BTreeSet;
use weavatrix_graph::NodeKind;

pub fn change_impact(state: &RepositoryState, args: &Value) -> Result<Value, String> {
    let git = super::history::changes(state, args)?;
    let files = changed_files(args, &git);
    let depth = arg_u64(args, "depth").unwrap_or(2);
    let max = arg_u64(args, "max_nodes").unwrap_or(40);
    let mut impacts = Vec::new();
    let mut seen = BTreeSet::new();
    for file in &files {
        let Some(node) = state
            .graph()
            .nodes()
            .iter()
            .find(|node| node.kind == NodeKind::File && node.label == *file)
        else {
            continue;
        };
        let result = super::graph::dependents(
            state,
            &json!({"label": node.id.as_str(), "depth": depth, "max_nodes": max}),
        )?;
        for dependent in result["dependents"].as_array().into_iter().flatten() {
            if let Some(id) = dependent["id"].as_str()
                && seen.insert(id.to_owned())
            {
                impacts.push(dependent.clone());
            }
        }
    }
    Ok(json!({
        "changed_files": files,
        "impacted_nodes": impacts,
        "git": git,
        "precision": "graph",
        "semantic_precision": "BOUNDED_STATIC",
        "coverage": "NOT_ASSUMED"
    }))
}

pub fn verified_change(state: &RepositoryState, args: &Value) -> Result<Value, String> {
    let task = arg_str(args, "task")?;
    let impact = change_impact(state, args);
    let audit = super::health::audit(state, &json!({"max_findings": 20}));
    let duplicates = if arg_bool(args, "duplicate_ratchet").unwrap_or(false) {
        super::health::duplicates(
            state,
            &json!({"mode": "exact", "top_n": 5, "min_tokens": 24}),
        )
        .map_or_else(
            |reason| json!({"state": "UNKNOWN", "reason": reason}),
            |value| json!({"state": "AVAILABLE", "report": value}),
        )
    } else {
        json!({"state": "SKIPPED"})
    };
    let (verdict, unknowns) = match &impact {
        Ok(_) if state.snapshot().diagnostics.is_empty() => (
            "UNKNOWN",
            vec![
                "static evidence cannot prove runtime behavior".to_owned(),
                "tests were not executed by the read-only MCP".to_owned(),
            ],
        ),
        Ok(_) => (
            "UNKNOWN",
            vec!["repository analysis contains diagnostics".to_owned()],
        ),
        Err(reason) => ("UNKNOWN", vec![reason.clone()]),
    };
    Ok(json!({
        "task": task,
        "verdict": verdict,
        "impact": impact.ok(),
        "audit": audit,
        "duplicates": duplicates,
        "unknowns": unknowns,
        "source_mutation": "NONE",
        "test_execution": "NONE"
    }))
}

pub fn trace_api(state: &RepositoryState, args: &Value) -> Result<Value, String> {
    let backend = arg_str(args, "backend")?;
    let backend_state = if same_root(state, backend) {
        None
    } else {
        Some(Weavatrix::open(backend).map_err(|error| error.to_string())?)
    };
    let backend = backend_state.as_ref().map_or(state, Weavatrix::state);
    let method = arg_str(args, "method").ok();
    let path_filter = arg_str(args, "path").ok();
    let endpoints = backend
        .graph()
        .nodes()
        .iter()
        .filter(|node| node.kind == NodeKind::Endpoint)
        .filter(|node| method.is_none_or(|method| node.label.starts_with(method)))
        .filter(|node| path_filter.is_none_or(|path| node.label.ends_with(path)))
        .collect::<Vec<_>>();
    let clients = args
        .get("clients")
        .and_then(Value::as_array)
        .ok_or_else(|| "clients must be an array".to_owned())?;
    let mut matches = Vec::new();
    for client in clients.iter().filter_map(Value::as_str) {
        let client_state = Weavatrix::open(client).map_err(|error| error.to_string())?;
        for endpoint in &endpoints {
            let route = endpoint
                .label
                .split_once(' ')
                .map_or(endpoint.label.as_str(), |(_, route)| route);
            let result = super::source::search(
                client_state.state(),
                &json!({"query": route, "max_results": 20}),
            )?;
            let count = result["occurrences"].as_u64().unwrap_or(0);
            if count > 0 {
                matches.push(json!({
                    "backend_endpoint": endpoint,
                    "client": client,
                    "occurrences": count,
                    "evidence": result["matches"]
                }));
            }
        }
    }
    Ok(json!({
        "backend": backend.root(),
        "clients": clients,
        "matches": matches,
        "unmatched_endpoints": endpoints.len().saturating_sub(matches.len()),
        "precision": "static endpoint plus exact literal client evidence",
        "dynamic_contracts": "UNKNOWN"
    }))
}

fn changed_files(args: &Value, git: &Value) -> Vec<String> {
    if let Some(files) = args.get("files").and_then(Value::as_array) {
        return files
            .iter()
            .filter_map(Value::as_str)
            .map(str::to_owned)
            .collect();
    }
    git["changes"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|change| change["path"].as_str())
        .map(str::to_owned)
        .collect()
}

fn same_root(state: &RepositoryState, value: &str) -> bool {
    state.root().to_string_lossy().eq_ignore_ascii_case(value)
        || state
            .root()
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| name == value)
}
