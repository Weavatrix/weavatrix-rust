//! Cross-repository API trace orchestration and caching.

mod contracts;
mod model;
mod pagination;
mod response;

use crate::engine::{RepositoryState, Weavatrix};
use blazingly_json::Value;
use std::collections::BTreeSet;

pub(in crate::operations) fn trace_api(
    state: &RepositoryState,
    args: &Value,
) -> Result<Value, String> {
    let mut engine = Weavatrix::from_state(state.clone());
    trace_api_cached(&mut engine, args)
}

pub(in crate::operations) fn trace_api_cached(
    engine: &mut Weavatrix,
    args: &Value,
) -> Result<Value, String> {
    let backend_selector = crate::operations::arg_str(args, "backend")?;
    let clients = clients(args)?;
    let backend_root = engine
        .ensure_repository_state(backend_selector)
        .map_err(|error| error.to_string())?;
    let client_roots = clients
        .iter()
        .map(|client| {
            engine
                .ensure_repository_state(client)
                .map(|root| ((*client).to_owned(), root))
                .map_err(|error| error.to_string())
        })
        .collect::<Result<Vec<_>, _>>()?;
    let backend = engine
        .known_state(&backend_root)
        .ok_or_else(|| format!("repository state not found: {}", backend_root.display()))?;
    let client_states = client_roots
        .iter()
        .map(|(name, root)| {
            engine
                .known_state(root)
                .map(|state| (name.clone(), state))
                .ok_or_else(|| format!("repository state not found: {}", root.display()))
        })
        .collect::<Result<Vec<_>, _>>()?;
    let cache_key = trace_api_cache_key(backend, &client_states, args)?;
    if let Some(key) = cache_key.as_deref()
        && let Some(cached) = engine.cached_tool_result(key)
    {
        return Ok(cached);
    }
    let result = trace_api_with_states(backend, &client_states, &clients, args)?;
    if let Some(key) = cache_key {
        engine.remember_tool_result(key, result.clone());
    }
    Ok(result)
}

fn clients(args: &Value) -> Result<BTreeSet<&str>, String> {
    let clients = args
        .get("clients")
        .and_then(Value::as_array)
        .ok_or_else(|| "clients must be an array".to_owned())?
        .iter()
        .map(|client| {
            client
                .as_str()
                .ok_or_else(|| "clients must contain only repository path strings".to_owned())
        })
        .collect::<Result<BTreeSet<_>, String>>()?;
    if clients.is_empty() {
        return Err("clients must contain at least one repository".to_owned());
    }
    if clients.len() > 20 {
        return Err("clients must contain at most 20 repositories".to_owned());
    }
    Ok(clients)
}

fn trace_api_cache_key(
    backend: &RepositoryState,
    clients: &[(String, &RepositoryState)],
    args: &Value,
) -> Result<Option<String>, String> {
    if args.get("runtime_evidence_files").is_some() {
        return Ok(None);
    }
    let mut key = blazingly_json::to_string(args)
        .map_err(|error| format!("could not serialize trace arguments: {error}"))?;
    key.push_str("\nbackend:");
    key.push_str(&backend.root().to_string_lossy());
    key.push(':');
    key.push_str(&backend.snapshot().revision);
    for (name, state) in clients {
        key.push_str("\nclient:");
        key.push_str(name);
        key.push(':');
        key.push_str(&state.root().to_string_lossy());
        key.push(':');
        key.push_str(&state.snapshot().revision);
    }
    Ok(Some(key))
}

fn trace_api_with_states(
    backend: &RepositoryState,
    client_states: &[(String, &RepositoryState)],
    clients: &BTreeSet<&str>,
    args: &Value,
) -> Result<Value, String> {
    let results = contracts::analyze(backend, client_states, args)?;
    let reasons = contracts::completeness_reasons(&results);
    let evidence = contracts::evidence(&results);
    let page = pagination::paginate(&evidence, args)?;
    let summary = response::summarize(&results);
    Ok(response::build(
        backend, clients, &results, &page, &reasons, &summary,
    ))
}
