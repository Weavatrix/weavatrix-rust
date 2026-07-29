use crate::tools::{arg_str, optional_bool, optional_str, optional_u64};
use crate::{RepositoryState, Weavatrix};
use blazingly_json::{Value, json};
use std::collections::BTreeSet;
use weavatrix_graph::{Direction, EdgeKind, NodeIndex, NodeKind};

pub fn change_impact(state: &RepositoryState, args: &Value) -> Result<Value, String> {
    let explicit_head = optional_str(args, "head_ref")?;
    let requested = explicit_changed_files(args)?;
    let (git, files) = if explicit_head.is_some() {
        let git = super::history::changes(state, args)?;
        let files = requested.unwrap_or_else(|| changed_files(&git));
        (git, files)
    } else {
        let base = optional_str(args, "base_ref")?
            .or(optional_str(args, "base")?)
            .unwrap_or("HEAD");
        let files =
            requested.map_or_else(|| super::history::worktree_changed_files(state, base), Ok)?;
        let git = json!({
            "base": base,
            "head": "WORKTREE",
            "changes": files.iter().map(|path| {
                json!({"path": path, "kind": "worktree"})
            }).collect::<Vec<_>>()
        });
        (git, files)
    };
    let depth = optional_u64(args, "depth")?.unwrap_or(2);
    let max = optional_u64(args, "max_nodes")?.unwrap_or(40);
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
        "status": "COMPLETE",
        "changed_files": files,
        "impacted_nodes": impacts,
        "git": git,
        "precision": "graph",
        "semantic_precision": "BOUNDED_STATIC",
        "coverage_evidence": {
            "present": false,
            "reason": "change_impact computes static graph reachability and does not consume measured test coverage"
        }
    }))
}

#[allow(clippy::too_many_lines)]
pub fn verified_change(state: &RepositoryState, args: &Value) -> Result<Value, String> {
    let task = arg_str(args, "task")?;
    let phase = optional_str(args, "phase")?.unwrap_or("plan");
    if !matches!(phase, "plan" | "verify") {
        return Err("phase must be plan or verify".to_owned());
    }
    let base_ref = optional_str(args, "base_ref")?.unwrap_or("HEAD");
    let impact = change_impact(state, args)?;
    let files = impact["changed_files"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .map(str::to_owned)
        .collect::<Vec<_>>();
    let (retrieval, seeds) = if phase == "verify" && files.is_empty() {
        (
            json!({
                "selected": [],
                "max_symbols": 0,
                "model": "no changed files; no edit context is required"
            }),
            Vec::new(),
        )
    } else {
        retrieve_change_context(state, task, &files, &impact, args)?
    };
    let edit_contexts = seeds
        .iter()
        .filter_map(|seed| state.graph().node_at(*seed))
        .filter(|node| node.span.is_some())
        .map(|node| {
            super::source::context(
                state,
                &json!({
                    "label": node.id.as_str(),
                    "max_related": 8,
                    "context_lines": 4
                }),
            )
            .map(|evidence| {
                json!({"symbol": node.id.as_str(), "status": "COMPLETE", "evidence": evidence})
            })
        })
        .collect::<Result<Vec<_>, String>>()?;
    let data_flow = data_flow_evidence(state, &seeds, args)?;

    let graph_baseline = if phase == "verify" {
        let graph_args = if let Some(head_ref) = optional_str(args, "head_ref")? {
            json!({"base_ref": base_ref, "head_ref": head_ref, "max_results": 100})
        } else {
            json!({"base_ref": base_ref, "max_results": 100})
        };
        json!({
            "state": "PASS",
            "evidence": super::history::graph_diff(state, &graph_args)?
        })
    } else {
        json!({"state": "PLANNED", "baseline": base_ref})
    };

    let audit = if phase == "verify" {
        super::health::audit(
            state,
            &json!({"max_findings": 50, "base_ref": base_ref, "debt": "new"}),
        )?
    } else {
        json!({"status": "PLANNED", "baseline": base_ref})
    };
    let architecture = if files.is_empty() {
        json!({"state": "NOT_APPLICABLE", "reason": "no changed files"})
    } else if phase == "verify" {
        super::architecture::verify(state)?
    } else {
        json!({
            "state": "PLANNED",
            "evidence": super::architecture::prepare(
                state,
                &json!({"intent": task, "files": files})
            )?
        })
    };

    let duplicate_ratchet = optional_bool(args, "duplicate_ratchet")?.unwrap_or(true);
    let duplicates = if !duplicate_ratchet {
        json!({"state": "SKIPPED", "enabled": false})
    } else if phase != "verify" {
        json!({"state": "PLANNED", "enabled": true})
    } else {
        let report = super::health::duplicates(
            state,
            &json!({"mode": "renamed", "top_n": 50, "min_tokens": 50}),
        )?;
        let families = report["families"].as_array().map_or(0, Vec::len);
        json!({
            "state": if families == 0 {"PASS"} else {"REVIEW"},
            "enabled": true,
            "reason": if families == 0 {
                Value::Null
            } else {
                json!("clone families exist; compare them with the immutable baseline before accepting the change")
            },
            "report": report
        })
    };

    let api_contract = if let Some(contract) = args.get("api_contract") {
        let evidence = trace_api(state, contract)?;
        let state = if evidence
            .pointer("/verdict/code")
            .and_then(Value::as_str)
            .is_some_and(|code| {
                matches!(
                    code,
                    "HTTP_METHOD_MISMATCH"
                        | "EVENT_CONTRACT_MISMATCH"
                        | "TYPED_API_CONTRACT_MISMATCH"
                )
            }) {
            "REVIEW"
        } else {
            "PASS"
        };
        json!({"state": state, "evidence": evidence})
    } else {
        json!({"state": "SKIPPED", "reason": "no api_contract scope was requested"})
    };
    let suggested_tests = suggested_tests(&impact);
    let requested_tests = args
        .get("tests")
        .map(|value| {
            value
                .as_array()
                .ok_or_else(|| "tests must be an array of strings".to_owned())?
                .iter()
                .map(|item| {
                    item.as_str()
                        .map(str::to_owned)
                        .ok_or_else(|| "tests must contain only strings".to_owned())
                })
                .collect::<Result<Vec<_>, String>>()
        })
        .transpose()?
        .unwrap_or_default();
    if optional_bool(args, "run_tests")?.unwrap_or(false) {
        return Err(
            "run_tests=true is invalid for the process-free verified_change tool; execute tests externally and supply their evidence"
                .to_owned(),
        );
    }
    let tests = if requested_tests.is_empty() {
        json!({
            "state": "COMPLETE",
            "requested": [],
            "suggested_files": suggested_tests,
            "execution": {
                "present": false,
                "reason": "no test command was requested"
            }
        })
    } else {
        json!({
            "state": "COMPLETE",
            "requested": requested_tests,
            "suggested_files": suggested_tests,
            "execution": {
                "present": false,
                "reason": "verified_change is process-free; execute the requested tests externally and attach their results"
            }
        })
    };

    let mut blockers = Vec::new();
    let mut limitations = Vec::new();
    if phase == "plan" {
        limitations.push(
            "verification has not run; apply the edit and call verified_change with phase=verify"
                .to_owned(),
        );
    } else if !files.is_empty() {
        if audit["status"] == "REVIEW"
            || audit
                .pointer("/debt/counts/new")
                .and_then(Value::as_u64)
                .is_some_and(|count| count > 0)
        {
            blockers.push("new Health findings or repository diagnostics were found".to_owned());
        }
        match architecture["state"].as_str().unwrap_or("FAILED") {
            "BLOCKED" => {
                blockers.push("new architecture-contract violations were found".to_owned());
            }
            "PASS" | "NOT_APPLICABLE" => {}
            _ => limitations.push(
                "architecture contract is not configured or verification is incomplete".to_owned(),
            ),
        }
        if duplicates["state"] == "REVIEW" {
            limitations.push("duplicate ratchet requires review".to_owned());
        }
        if api_contract["state"] == "REVIEW" {
            blockers.push("cross-repository API contract mismatches were found".to_owned());
        }
        if !requested_tests.is_empty() {
            limitations
                .push("requested tests were not executed by the process-free core".to_owned());
        } else if !suggested_tests.is_empty() {
            limitations.push("affected tests were identified but not executed".to_owned());
        }
    }
    let verdict = if blockers.is_empty() {
        if phase == "plan" {
            "PLANNED"
        } else if limitations.is_empty() {
            "PASS"
        } else {
            "REVIEW"
        }
    } else {
        "BLOCKED"
    };

    Ok(json!({
        "schemaVersion": "weavatrix.verified-change.v1",
        "status": "COMPLETE",
        "task": task,
        "phase": phase,
        "verdict": verdict,
        "blockers": blockers,
        "impact": impact,
        "changeImpact": impact,
        "retrieval": retrieval,
        "editContexts": edit_contexts,
        "dataFlow": data_flow,
        "graphBaseline": graph_baseline,
        "architecture": architecture,
        "audit": audit,
        "duplicates": duplicates,
        "apiContract": api_contract,
        "tests": tests,
        "limitations": limitations,
        "source_mutation": "NONE",
        "test_execution": tests["execution"].clone()
    }))
}

fn retrieve_change_context(
    state: &RepositoryState,
    task: &str,
    files: &[String],
    impact: &Value,
    args: &Value,
) -> Result<(Value, Vec<NodeIndex>), String> {
    let max = usize::try_from(optional_u64(args, "max_symbols")?.unwrap_or(12))
        .map_err(|_| "max_symbols is too large".to_owned())?
        .clamp(1, 50);
    let changed = files.iter().map(String::as_str).collect::<BTreeSet<_>>();
    let terms = task
        .split(|character: char| !character.is_alphanumeric() && character != '_')
        .filter(|term| term.len() >= 3)
        .map(str::to_ascii_lowercase)
        .collect::<BTreeSet<_>>();
    let mut selected = Vec::new();
    let mut seen = BTreeSet::new();
    let mut push = |index: NodeIndex| {
        if selected.len() < max && seen.insert(index) {
            selected.push(index);
        }
    };
    for id in impact["impacted_nodes"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|node| node["id"].as_str())
    {
        if let Some(index) = state.graph().node_index(id) {
            push(index);
        }
    }
    for (slot, node) in state.graph().nodes().iter().enumerate() {
        if node
            .span
            .as_ref()
            .is_some_and(|span| changed.contains(span.file.as_str()))
        {
            push(NodeIndex::new(u32::try_from(slot).unwrap_or(u32::MAX)));
        }
    }
    for (slot, node) in state.graph().nodes().iter().enumerate() {
        let text = format!(
            "{} {}",
            node.label,
            node.span.as_ref().map_or("", |span| span.file.as_str())
        )
        .to_ascii_lowercase();
        if !terms.is_empty() && terms.iter().any(|term| text.contains(term)) {
            push(NodeIndex::new(u32::try_from(slot).unwrap_or(u32::MAX)));
        }
    }
    let evidence = selected
        .iter()
        .filter_map(|index| state.graph().node_at(*index))
        .collect::<Vec<_>>();
    Ok((
        json!({
            "selected": evidence,
            "max_symbols": max,
            "model": "changed declarations, graph blast radius and task-term matches"
        }),
        selected,
    ))
}

fn data_flow_evidence(
    state: &RepositoryState,
    seeds: &[NodeIndex],
    args: &Value,
) -> Result<Value, String> {
    let depth = usize::try_from(optional_u64(args, "data_flow_depth")?.unwrap_or(2))
        .map_err(|_| "data_flow_depth is too large".to_owned())?
        .clamp(1, 3);
    let max = usize::try_from(optional_u64(args, "max_data_flow_edges")?.unwrap_or(30))
        .map_err(|_| "max_data_flow_edges is too large".to_owned())?
        .clamp(1, 60);
    let relations = [EdgeKind::Calls.as_str(), EdgeKind::References.as_str()]
        .into_iter()
        .map(str::to_owned)
        .collect::<BTreeSet<_>>();
    let (_, traversed) = super::graph_walk::traverse(
        state,
        seeds.to_vec(),
        depth,
        max.saturating_mul(2).saturating_add(seeds.len()),
        Direction::Both,
        false,
        Some(&relations),
    );
    let total = traversed.len();
    let edges = traversed
        .into_iter()
        .filter_map(|index| state.graph().edge_at(index))
        .take(max)
        .collect::<Vec<_>>();
    Ok(json!({
        "status": "COMPLETE",
        "model": "bounded call/reference graph evidence; not CFG or taint analysis",
        "depth": depth,
        "edges": edges,
        "total_edges": total,
        "capped": total > max
    }))
}

fn suggested_tests(impact: &Value) -> Vec<String> {
    impact["impacted_nodes"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|node| {
            node["span"]["file"]
                .as_str()
                .or_else(|| node["source_file"].as_str())
        })
        .filter(|path| {
            let lower = path.to_ascii_lowercase();
            lower.contains("/test")
                || lower.contains("\\test")
                || lower.contains(".test.")
                || lower.contains(".spec.")
                || lower.ends_with("_test.go")
        })
        .map(str::to_owned)
        .collect::<BTreeSet<_>>()
        .into_iter()
        .take(30)
        .collect()
}

#[allow(clippy::too_many_lines)]
pub fn trace_api(state: &RepositoryState, args: &Value) -> Result<Value, String> {
    let mut engine = Weavatrix::from_state(state.clone());
    trace_api_cached(&mut engine, args)
}

pub fn trace_api_cached(engine: &mut Weavatrix, args: &Value) -> Result<Value, String> {
    let backend_selector = arg_str(args, "backend")?;
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

#[allow(clippy::too_many_lines)]
fn trace_api_with_states(
    backend: &RepositoryState,
    client_states: &[(String, &RepositoryState)],
    clients: &BTreeSet<&str>,
    args: &Value,
) -> Result<Value, String> {
    let transport = optional_str(args, "transport")?.unwrap_or("all");
    if !matches!(transport, "all" | "http" | "graphql" | "grpc" | "event") {
        return Err("transport must be all, http, graphql, grpc, or event".to_owned());
    }

    if args.get("runtime_config").is_some() {
        return Err(
            "runtime_config is not revision-bound evidence; use runtime_evidence_files".to_owned(),
        );
    }

    let mut reasons = Vec::new();
    let http = if matches!(transport, "all" | "http") {
        http_contracts(backend, client_states, args)?
    } else {
        json!({
            "status": "NOT_APPLICABLE",
            "totals": {"endpoints": 0, "matches": 0, "method_mismatches": 0},
            "contracts": []
        })
    };
    let events = if matches!(transport, "all" | "event") {
        event_contracts(backend, client_states, args)?
    } else {
        json!({
            "status": "NOT_APPLICABLE",
            "totals": {"contracts": 0, "matches": 0},
            "contracts": []
        })
    };
    let graphql = if matches!(transport, "all" | "graphql") {
        typed_api_contracts(backend, client_states, args, "graphql")?
    } else {
        empty_typed_contracts("graphql")
    };
    let grpc = if matches!(transport, "all" | "grpc") {
        typed_api_contracts(backend, client_states, args, "grpc")?
    } else {
        empty_typed_contracts("grpc")
    };
    reasons.extend(
        events
            .pointer("/completeness/reasons")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
            .map(str::to_owned),
    );
    for result in [&graphql, &grpc] {
        reasons.extend(
            result
                .pointer("/completeness/reasons")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .filter_map(Value::as_str)
                .map(str::to_owned),
        );
    }
    let mut evidence = http["contracts"]
        .as_array()
        .into_iter()
        .flatten()
        .cloned()
        .chain(
            events["contracts"]
                .as_array()
                .into_iter()
                .flatten()
                .cloned(),
        )
        .chain(
            graphql["contracts"]
                .as_array()
                .into_iter()
                .flatten()
                .cloned(),
        )
        .chain(grpc["contracts"].as_array().into_iter().flatten().cloned())
        .collect::<Vec<_>>();
    evidence.sort_by(|left, right| {
        left["key"]
            .as_str()
            .unwrap_or_default()
            .cmp(right["key"].as_str().unwrap_or_default())
    });
    let page_size = usize::try_from(optional_u64(args, "page_size")?.unwrap_or(10))
        .map_err(|_| "page_size is too large".to_owned())?
        .clamp(1, 50);
    let offset = match cursor_offset(args) {
        Ok(offset) if offset <= evidence.len() => offset,
        Ok(_) => return Err("cursor offset is outside the current evidence set".to_owned()),
        Err(reason) => return Err(reason),
    };
    let end = offset.saturating_add(page_size).min(evidence.len());
    let page = evidence[offset..end].to_vec();
    let next_cursor = (end < evidence.len()).then(|| format!("v1:{end}"));
    let http_mismatches = http
        .pointer("/totals/method_mismatches")
        .and_then(Value::as_u64)
        .unwrap_or(0);
    let event_mismatches = events
        .pointer("/totals/mismatches")
        .and_then(Value::as_u64)
        .unwrap_or(0);
    let typed_mismatches = graphql
        .pointer("/totals/mismatches")
        .and_then(Value::as_u64)
        .unwrap_or(0)
        + grpc
            .pointer("/totals/mismatches")
            .and_then(Value::as_u64)
            .unwrap_or(0);
    let matched = http
        .pointer("/totals/matches")
        .and_then(Value::as_u64)
        .unwrap_or(0)
        + events
            .pointer("/totals/matches")
            .and_then(Value::as_u64)
            .unwrap_or(0)
        + graphql
            .pointer("/totals/matches")
            .and_then(Value::as_u64)
            .unwrap_or(0)
        + grpc
            .pointer("/totals/matches")
            .and_then(Value::as_u64)
            .unwrap_or(0);
    let verdict = if typed_mismatches > 0 {
        "TYPED_API_CONTRACT_MISMATCH"
    } else if event_mismatches > 0 {
        "EVENT_CONTRACT_MISMATCH"
    } else if http_mismatches > 0 {
        "HTTP_METHOD_MISMATCH"
    } else if matched > 0 {
        "MATCHED"
    } else {
        "NO_STATIC_CLIENT_MATCH"
    };
    let complete = reasons.is_empty();
    let response_detail = optional_str(args, "response_detail")?.unwrap_or("compact");
    if !matches!(response_detail, "compact" | "full") {
        return Err("response_detail must be compact or full".to_owned());
    }
    Ok(json!({
        "crossRepoContractV": 1,
        "status": "COMPLETE",
        "verdict": {
            "code": verdict,
            "method_mismatches": http_mismatches,
            "event_mismatches": event_mismatches,
            "typed_contract_mismatches": typed_mismatches,
            "matched_contracts": matched
        },
        "repositories": {
            "backend": backend.root(),
            "clients": clients
        },
        "transport": transport,
        "http": http,
        "graphql": graphql,
        "grpc": grpc,
        "transport_contracts": events,
        "matches": http["contracts"]
            .as_array()
            .into_iter()
            .flatten()
            .cloned()
            .chain(events["matches"].as_array().into_iter().flatten().cloned())
            .chain(graphql["matches"].as_array().into_iter().flatten().cloned())
            .chain(grpc["matches"].as_array().into_iter().flatten().cloned())
            .collect::<Vec<_>>(),
        "unmatched_endpoints": http
            .pointer("/totals/unmatched_endpoints")
            .and_then(Value::as_u64)
            .unwrap_or(0)
            + graphql.pointer("/totals/unmatched_endpoints").and_then(Value::as_u64).unwrap_or(0)
            + grpc.pointer("/totals/unmatched_endpoints").and_then(Value::as_u64).unwrap_or(0),
        "evidencePage": {
            "detail": response_detail,
            "offset": offset,
            "page_size": page_size,
            "total_items": evidence.len(),
            "returned_items": page.len(),
            "has_more": end < evidence.len(),
            "next_cursor": next_cursor,
            "items": page
        },
        "completeness": {
            "complete": complete,
            "status": "COMPLETE",
            "reasons": reasons
        },
        "precision": "lossless-parser-derived GraphQL/protobuf contracts, exact typed graph matches, event semantics, and exact or template-prefix HTTP literals",
        "dynamic_contracts": {
            "evaluated": true,
            "method": "bounded static candidates plus revision-bound runtime evidence when supplied"
        },
        "source_mutation": "NONE",
        "network": "NONE"
    }))
}

#[allow(clippy::too_many_lines)]
fn http_contracts(
    backend: &RepositoryState,
    clients: &[(String, &RepositoryState)],
    args: &Value,
) -> Result<Value, String> {
    let method = optional_str(args, "method")?;
    let path_filter = optional_str(args, "path")?;
    let changed_files = args
        .get("changed_files")
        .map(|value| {
            value
                .as_array()
                .ok_or_else(|| "changed_files must be an array of strings".to_owned())?
                .iter()
                .map(|item| {
                    item.as_str()
                        .map(normalize_path)
                        .ok_or_else(|| "changed_files must contain only strings".to_owned())
                })
                .collect::<Result<BTreeSet<_>, String>>()
        })
        .transpose()?
        .unwrap_or_default();
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
        .filter(|(slot, _)| super::node_is_visible(backend, *slot, args))
        .filter(|(_, node)| {
            changed_files.is_empty()
                || node
                    .span
                    .as_ref()
                    .is_some_and(|span| changed_files.contains(&normalize_path(&span.file)))
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
        let mut callsites = Vec::new();
        let mut affected = BTreeSet::new();
        let mut endpoint_calls = 0_usize;
        for (client, client_state) in clients {
            let result = super::source::search(
                client_state,
                &json!({"query": query, "max_results": max_matches}),
            )?;
            for evidence in result["matches"]
                .as_array()
                .into_iter()
                .flatten()
                .filter(|evidence| {
                    evidence["path"]
                        .as_str()
                        .is_some_and(|path| super::health::path_is_visible(path, args))
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
                method_mismatches += usize::from(mismatch);
                total_calls += 1;
                endpoint_calls += 1;
                if let Some(path) = evidence["path"].as_str() {
                    affected.insert(path.to_owned());
                }
                if callsites.len() < per_item {
                    callsites.push(json!({
                        "client": client,
                        "file": evidence["path"].clone(),
                        "line": evidence["line"].clone(),
                        "method": call_method,
                        "method_mismatch": mismatch,
                        "match": if evidence["text"]
                            .as_str()
                            .is_some_and(|line| line.contains(route))
                        {
                            "EXACT_LITERAL"
                        } else {
                            "TEMPLATE_PREFIX"
                        },
                        "text": evidence["text"].clone()
                    }));
                }
            }
        }
        if affected.is_empty() {
            unmatched += 1;
        } else {
            contracts.push(json!({
                "key": format!("http:{}:{route}", backend_method),
                "transport": "http",
                "method": backend_method,
                "path": route,
                "backend_endpoint": endpoint,
                "callsites": callsites,
                "affected_files": affected,
                "callsite_count": endpoint_calls
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

fn empty_typed_contracts(transport: &str) -> Value {
    json!({
        "status": "NOT_APPLICABLE",
        "transport": transport,
        "typedContractsV": 1,
        "totals": {
            "endpoints": 0,
            "client_contracts": 0,
            "matches": 0,
            "mismatches": 0,
            "unmatched_endpoints": 0,
            "parser_diagnostics": 0
        },
        "contracts": [],
        "matches": [],
        "mismatches": [],
        "completeness": {"complete": true, "reasons": []}
    })
}

/// Matches typed provider contracts to typed client operations/declarations.
///
/// GraphQL clients contribute parser-derived `Calls` edges. A protobuf file is
/// itself the gRPC wire contract, so a client-side service declaration is also
/// valid compatibility evidence; its exact RPC and streaming signature must
/// equal the provider declaration.
#[allow(clippy::too_many_lines)]
fn typed_api_contracts(
    backend: &RepositoryState,
    clients: &[(String, &RepositoryState)],
    args: &Value,
    transport: &str,
) -> Result<Value, String> {
    let max_endpoints = usize::try_from(optional_u64(args, "max_endpoints")?.unwrap_or(250))
        .map_err(|_| "max_endpoints is too large".to_owned())?
        .clamp(1, 500);
    let per_item = usize::try_from(optional_u64(args, "per_item_limit")?.unwrap_or(8))
        .map_err(|_| "per_item_limit is too large".to_owned())?
        .clamp(1, 50);
    let all_providers = typed_nodes(backend, args, transport)
        .into_iter()
        .filter(|(_, node)| {
            backend
                .graph()
                .incoming(&node.id)
                .any(|edge| edge.kind == EdgeKind::Exposes)
        })
        .collect::<Vec<_>>();
    let bound_reached = all_providers.len() > max_endpoints;
    let providers = all_providers
        .into_iter()
        .take(max_endpoints)
        .collect::<Vec<_>>();
    let provider_keys = providers
        .iter()
        .map(|(_, node)| node.label.clone())
        .collect::<BTreeSet<_>>();
    let provider_identities = provider_keys
        .iter()
        .filter_map(|label| typed_identity(label, transport))
        .collect::<BTreeSet<_>>();

    let mut contracts = Vec::new();
    let mut matched_contracts = Vec::new();
    let mut mismatches = Vec::new();
    let mut client_contracts = BTreeSet::new();
    let mut unmatched = 0_usize;
    for (_, endpoint) in &providers {
        let backend_evidence =
            typed_evidence(backend, "backend", endpoint, &[EdgeKind::Exposes], per_item);
        let mut client_evidence = Vec::new();
        for (repository, client_state) in clients {
            for (_, candidate) in typed_nodes(client_state, args, transport)
                .into_iter()
                .filter(|(_, node)| node.label == endpoint.label)
            {
                let relations = typed_client_relations(transport);
                let evidence =
                    typed_evidence(client_state, repository, candidate, relations, per_item);
                if !evidence.is_empty() {
                    client_contracts.insert((repository.clone(), candidate.label.clone()));
                    client_evidence.extend(evidence);
                }
            }
        }
        client_evidence.truncate(per_item);
        let matched = !client_evidence.is_empty();
        unmatched += usize::from(!matched);
        let contract = json!({
            "key": typed_key(&endpoint.label, transport),
            "transport": transport,
            "signature": endpoint.label,
            "backend": backend_evidence,
            "clients": client_evidence,
            "matched": matched
        });
        if matched {
            matched_contracts.push(contract.clone());
        }
        contracts.push(contract);
    }

    // Same operation/RPC identity but a different operation kind or streaming
    // mode is stronger evidence than "not found": report the exact signature
    // disagreement instead of flattening it into an unmatched endpoint.
    for (repository, client_state) in clients {
        for (_, endpoint) in typed_nodes(client_state, args, transport) {
            let relations = typed_client_relations(transport);
            let evidence = typed_evidence(client_state, repository, endpoint, relations, per_item);
            if evidence.is_empty() {
                continue;
            }
            client_contracts.insert((repository.clone(), endpoint.label.clone()));
            if provider_keys.contains(&endpoint.label) {
                continue;
            }
            let identity = typed_identity(&endpoint.label, transport);
            let providers_with_identity = providers
                .iter()
                .filter(|(_, provider)| typed_identity(&provider.label, transport) == identity)
                .map(|(_, provider)| provider.label.clone())
                .collect::<Vec<_>>();
            let (kind, expected) = if providers_with_identity.is_empty()
                || identity
                    .as_ref()
                    .is_none_or(|identity| !provider_identities.contains(identity))
            {
                ("MISSING_PROVIDER", Vec::new())
            } else {
                (typed_mismatch_kind(transport), providers_with_identity)
            };
            mismatches.push(json!({
                "key": typed_key(&endpoint.label, transport),
                "transport": transport,
                "kind": kind,
                "identity": identity,
                "expected": expected,
                "actual": endpoint.label,
                "client": repository,
                "evidence": evidence
            }));
        }
    }
    mismatches.sort_by(|left, right| {
        left["key"]
            .as_str()
            .unwrap_or_default()
            .cmp(right["key"].as_str().unwrap_or_default())
    });
    mismatches.dedup();

    let mut parser_diagnostics = typed_diagnostics(backend, "backend", transport);
    for (repository, client_state) in clients {
        parser_diagnostics.extend(typed_diagnostics(client_state, repository, transport));
    }
    if !parser_diagnostics.is_empty() {
        return Err(format!(
            "{transport} contract parsing failed closed: {}",
            parser_diagnostics.join("; ")
        ));
    }
    Ok(json!({
        "status": "COMPLETE",
        "transport": transport,
        "typedContractsV": 1,
        "totals": {
            "endpoints": providers.len(),
            "client_contracts": client_contracts.len(),
            "matches": matched_contracts.len(),
            "mismatches": mismatches.len(),
            "unmatched_endpoints": unmatched,
            "parser_diagnostics": 0
        },
        "contracts": contracts,
        "matches": matched_contracts,
        "mismatches": mismatches,
        "completeness": {
            "complete": true,
            "status": "COMPLETE",
            "reasons": []
        },
        "bounds": {
            "reached": bound_reached,
            "max_endpoints": max_endpoints,
            "reason": if bound_reached {
                json!("typed contract result is capped at max_endpoints")
            } else {
                Value::Null
            }
        },
        "precision": "exact typed endpoint signatures and parsed edge provenance",
        "dynamic_dispatch": {
            "evaluated": true,
            "method": "exact typed operation and RPC identities"
        },
        "network": "NONE",
        "process": "NONE",
        "source_mutation": "NONE"
    }))
}

fn typed_nodes<'state>(
    state: &'state RepositoryState,
    args: &Value,
    transport: &str,
) -> Vec<(usize, &'state weavatrix_graph::Node)> {
    state
        .graph()
        .nodes()
        .iter()
        .enumerate()
        .filter(|(_, node)| node.kind == NodeKind::Endpoint)
        .filter(|(_, node)| typed_identity(&node.label, transport).is_some())
        .filter(|(slot, _)| super::node_is_visible(state, *slot, args))
        .collect()
}

fn typed_client_relations(transport: &str) -> &'static [EdgeKind] {
    if transport == "grpc" {
        &[EdgeKind::Calls, EdgeKind::Exposes]
    } else {
        &[EdgeKind::Calls]
    }
}

fn typed_evidence(
    state: &RepositoryState,
    repository: &str,
    endpoint: &weavatrix_graph::Node,
    relations: &[EdgeKind],
    limit: usize,
) -> Vec<Value> {
    state
        .graph()
        .incoming(&endpoint.id)
        .filter(|edge| relations.contains(&edge.kind))
        .filter_map(|edge| {
            let source = state.graph().node(edge.source.as_str())?;
            let span = edge.provenance.span.as_ref();
            Some(json!({
                "repository": repository,
                "role": match edge.kind {
                    EdgeKind::Exposes => "declares",
                    EdgeKind::Calls => "calls",
                    _ => "references"
                },
                "endpoint_id": endpoint.id.as_str(),
                "source_id": source.id.as_str(),
                "source_label": source.label.as_str(),
                "relation": edge.kind.as_str(),
                "extractor": edge.provenance.extractor.as_str(),
                "confidence": format!("{:?}", edge.provenance.confidence).to_ascii_lowercase(),
                "file": span.map(|span| span.file.as_str()),
                "line": span.map(|span| span.start.line),
                "column": span.map(|span| span.start.column)
            }))
        })
        .take(limit)
        .collect()
}

fn typed_identity(label: &str, transport: &str) -> Option<String> {
    match transport {
        "graphql" => {
            let remainder = label.strip_prefix("GRAPHQL ")?;
            let (_, field) = remainder.split_once(' ')?;
            Some(format!("graphql:{field}"))
        }
        "grpc" => {
            let remainder = label.strip_prefix("GRPC ")?;
            let rpc = remainder
                .rsplit_once(" [")
                .map_or(remainder, |(rpc, _)| rpc);
            Some(format!("grpc:{rpc}"))
        }
        _ => None,
    }
}

fn typed_key(label: &str, transport: &str) -> String {
    format!(
        "{transport}:{}",
        label.to_ascii_lowercase().replace(' ', ":")
    )
}

fn typed_mismatch_kind(transport: &str) -> &'static str {
    if transport == "grpc" {
        "STREAMING_MODE_MISMATCH"
    } else {
        "GRAPHQL_OPERATION_MISMATCH"
    }
}

fn typed_diagnostics(state: &RepositoryState, repository: &str, transport: &str) -> Vec<String> {
    let prefix = if transport == "grpc" {
        "protobuf."
    } else {
        "graphql."
    };
    state
        .snapshot()
        .diagnostics
        .iter()
        .filter(|diagnostic| diagnostic.code.starts_with(prefix))
        .map(|diagnostic| {
            let location = diagnostic.span.as_ref().map_or_else(
                || repository.to_owned(),
                |span| format!("{repository}:{}:{}", span.file, span.start.line),
            );
            format!("{location}: {}: {}", diagnostic.code, diagnostic.message)
        })
        .collect()
}

fn event_contracts(
    backend: &RepositoryState,
    clients: &[(String, &RepositoryState)],
    args: &Value,
) -> Result<Value, String> {
    super::transport_contracts::event_contracts(backend, clients, args)
}

fn route_query(route: &str) -> &str {
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

fn route_matches(route: &str, line: &str) -> bool {
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

fn infer_http_method(line: &str) -> Option<&'static str> {
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
    ] {
        if lower.contains(needle) {
            return Some(method);
        }
    }
    lower.contains("fetch(").then_some("GET")
}

fn cursor_offset(args: &Value) -> Result<usize, String> {
    let Some(cursor) = optional_str(args, "cursor")? else {
        return Ok(0);
    };
    let Some(value) = cursor.strip_prefix("v1:") else {
        return Err("cursor format is invalid; expected v1:<offset>".to_owned());
    };
    value
        .parse::<usize>()
        .map_err(|_| "cursor offset is invalid".to_owned())
}

fn explicit_changed_files(args: &Value) -> Result<Option<Vec<String>>, String> {
    if let Some(value) = args.get("files") {
        let files = value
            .as_array()
            .ok_or_else(|| "files must be an array of strings".to_owned())?;
        return Ok(Some(
            files
                .iter()
                .map(|item| {
                    item.as_str()
                        .map(normalize_path)
                        .ok_or_else(|| "files must contain only strings".to_owned())
                })
                .collect::<Result<Vec<_>, String>>()?,
        ));
    }
    let Some(diff) = optional_str(args, "diff")? else {
        return Ok(None);
    };
    let mut files = BTreeSet::new();
    for line in diff.lines() {
        let candidate = line
            .strip_prefix("+++ ")
            .or_else(|| line.strip_prefix("--- "))
            .or_else(|| {
                line.strip_prefix("diff --git ")
                    .and_then(|rest| rest.split_whitespace().nth(1))
            });
        let Some(path) = candidate else {
            continue;
        };
        let path = path.split('\t').next().unwrap_or(path);
        if path != "/dev/null" {
            let path = path
                .strip_prefix("a/")
                .or_else(|| path.strip_prefix("b/"))
                .unwrap_or(path);
            files.insert(normalize_path(path));
        }
    }
    Ok(Some(files.into_iter().collect()))
}

fn changed_files(git: &Value) -> Vec<String> {
    git["changes"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|change| change["path"].as_str())
        .map(normalize_path)
        .collect()
}

fn normalize_path(path: &str) -> String {
    path.replace('\\', "/")
}
