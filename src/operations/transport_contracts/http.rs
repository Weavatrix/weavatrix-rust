use super::{BTreeSet, NodeKind, RepositoryState, Value, json, optional_str, optional_u64};

pub(in crate::operations) fn http_contracts(
    backend: &RepositoryState,
    clients: &[(String, &RepositoryState)],
    args: &Value,
) -> Result<Value, String> {
    let method = optional_str(args, "method")?;
    let path_filter = optional_str(args, "path")?;
    let changed_files = changed_http_files(args)?;
    let max_endpoints = usize::try_from(optional_u64(args, "max_endpoints")?.unwrap_or(250))
        .map_err(|_| "max_endpoints is too large".to_owned())?
        .clamp(1, 500);
    let endpoints = backend
        .graph()
        .nodes()
        .iter()
        .enumerate()
        .filter(|(_, node)| node.kind == NodeKind::Endpoint)
        .filter(|(_, node)| !node.label.starts_with("GRAPHQL ") && !node.label.starts_with("GRPC "))
        .filter(|(_, node)| method.is_none_or(|method| node.label.starts_with(method)))
        .filter(|(_, node)| path_filter.is_none_or(|path| node.label.ends_with(path)))
        .filter(|(slot, _)| super::super::node_is_visible(backend, *slot, args))
        .filter(|(_, node)| {
            changed_files.is_empty()
                || node
                    .span
                    .as_ref()
                    .is_some_and(|span| changed_files.contains(&normalize_http_path(&span.file)))
        })
        .take(max_endpoints)
        .map(|(_, node)| node)
        .collect::<Vec<_>>();
    let per_item = usize::try_from(optional_u64(args, "per_item_limit")?.unwrap_or(5))
        .map_err(|_| "per_item_limit is too large".to_owned())?
        .clamp(1, 25);
    let max_matches = usize::try_from(optional_u64(args, "max_matches")?.unwrap_or(1_000))
        .map_err(|_| "max_matches is too large".to_owned())?
        .clamp(1, 5_000);
    let mut contracts = Vec::new();
    let mut total_calls = 0_usize;
    let mut method_mismatches = 0_usize;
    let mut unmatched = 0_usize;
    for endpoint in &endpoints {
        let (backend_method, route) = endpoint
            .label
            .split_once(' ')
            .unwrap_or(("ANY", endpoint.label.as_str()));
        let query = route_query(route);
        if query.is_empty() {
            // A root route has no selective literal to search for. Passing an
            // empty query to the search engine is a tool error; searching for
            // "/" would instead manufacture thousands of unrelated matches.
            // Keep it as an explicit unmatched endpoint.
            unmatched += 1;
            continue;
        }
        let calls =
            find_endpoint_calls(route, backend_method, clients, args, per_item, max_matches)?;
        method_mismatches += calls.method_mismatches;
        total_calls += calls.total;
        if calls.affected_files.is_empty() {
            unmatched += 1;
        } else {
            contracts.push(json!({
                "key": format!("http:{}:{route}", backend_method),
                "transport": "http",
                "method": backend_method,
                "path": route,
                "backend_endpoint": endpoint,
                "callsites": calls.callsites,
                "affected_files": calls.affected_files,
                "callsite_count": calls.total
            }));
        }
    }
    Ok(json!({
        "status": "COMPLETE",
        "totals": {
            "endpoints": endpoints.len(),
            "matches": contracts.len(),
            "callsites": total_calls,
            "method_mismatches": method_mismatches,
            "unmatched_endpoints": unmatched
        },
        "contracts": contracts
    }))
}

struct EndpointCalls {
    callsites: Vec<Value>,
    affected_files: BTreeSet<String>,
    total: usize,
    method_mismatches: usize,
}

fn changed_http_files(args: &Value) -> Result<BTreeSet<String>, String> {
    args.get("changed_files")
        .map(|value| {
            value
                .as_array()
                .ok_or_else(|| "changed_files must be an array of strings".to_owned())?
                .iter()
                .map(|item| {
                    item.as_str()
                        .map(normalize_http_path)
                        .ok_or_else(|| "changed_files must contain only strings".to_owned())
                })
                .collect()
        })
        .transpose()
        .map(Option::unwrap_or_default)
}

fn find_endpoint_calls(
    route: &str,
    backend_method: &str,
    clients: &[(String, &RepositoryState)],
    args: &Value,
    per_item: usize,
    max_matches: usize,
) -> Result<EndpointCalls, String> {
    let mut calls = EndpointCalls {
        callsites: Vec::new(),
        affected_files: BTreeSet::new(),
        total: 0,
        method_mismatches: 0,
    };
    for (client, client_state) in clients {
        let result = super::super::source::search(
            client_state,
            &json!({"query": route_query(route), "max_results": max_matches}),
        )?;
        for evidence in result["matches"]
            .as_array()
            .into_iter()
            .flatten()
            .filter(|evidence| {
                evidence["path"]
                    .as_str()
                    .is_some_and(|path| super::super::health::path_is_visible(path, args))
            })
            .filter(|evidence| {
                evidence["text"]
                    .as_str()
                    .is_some_and(|line| route_matches(route, line))
            })
        {
            let call_method = evidence["text"].as_str().and_then(infer_http_method);
            let mismatch = call_method.is_some_and(|method| {
                backend_method != "ANY" && backend_method != "ALL" && method != backend_method
            });
            calls.method_mismatches += usize::from(mismatch);
            calls.total += 1;
            if let Some(path) = evidence["path"].as_str() {
                calls.affected_files.insert(path.to_owned());
            }
            if calls.callsites.len() < per_item {
                calls.callsites.push(json!({
                    "client": client,
                    "file": evidence["path"].clone(),
                    "line": evidence["line"].clone(),
                    "method": call_method,
                    "method_mismatch": mismatch,
                    "match": if evidence["text"].as_str().is_some_and(|line| line.contains(route)) {
                        "EXACT_LITERAL"
                    } else {
                        "TEMPLATE_PREFIX"
                    },
                    "text": evidence["text"].clone()
                }));
            }
        }
    }
    Ok(calls)
}

pub(super) fn route_query(route: &str) -> &str {
    let boundary = route
        .char_indices()
        .find(|(_, character)| matches!(character, ':' | '{' | '$' | '*'))
        .map_or(route.len(), |(index, _)| index);
    let prefix = &route[..boundary];
    prefix.trim_end_matches('/').rsplit_once('/').map_or_else(
        || prefix.trim_end_matches('/'),
        |(_, tail)| {
            if tail.is_empty() {
                prefix.trim_end_matches('/')
            } else {
                prefix
            }
        },
    )
}

pub(super) fn route_matches(route: &str, line: &str) -> bool {
    if line.contains(route) {
        return true;
    }
    let static_parts = route
        .split('/')
        .filter(|part| {
            !part.is_empty()
                && !part.starts_with(':')
                && !part.starts_with('{')
                && !part.contains('$')
                && *part != "*"
        })
        .collect::<Vec<_>>();
    !static_parts.is_empty() && static_parts.iter().all(|part| line.contains(part))
}

pub(super) fn infer_http_method(line: &str) -> Option<&'static str> {
    let lower = line.to_ascii_lowercase();
    for (needle, method) in [
        (".delete(", "DELETE"),
        (".patch(", "PATCH"),
        (".post(", "POST"),
        (".put(", "PUT"),
        (".head(", "HEAD"),
        (".options(", "OPTIONS"),
        (".get(", "GET"),
        ("method: 'delete'", "DELETE"),
        ("method: \"delete\"", "DELETE"),
        ("method: 'patch'", "PATCH"),
        ("method: \"patch\"", "PATCH"),
        ("method: 'post'", "POST"),
        ("method: \"post\"", "POST"),
        ("method: 'put'", "PUT"),
        ("method: \"put\"", "PUT"),
        ("method: 'get'", "GET"),
        ("method: \"get\"", "GET"),
        ("httpmethod = \"delete\"", "DELETE"),
        ("httpmethod = \"patch\"", "PATCH"),
        ("httpmethod = \"post\"", "POST"),
        ("httpmethod = \"put\"", "PUT"),
        ("httpmethod = \"get\"", "GET"),
    ] {
        if lower.contains(needle) {
            return Some(method);
        }
    }
    lower.contains("fetch(").then_some("GET")
}

fn normalize_http_path(path: &str) -> String {
    path.replace('\\', "/").trim_start_matches("./").to_owned()
}
