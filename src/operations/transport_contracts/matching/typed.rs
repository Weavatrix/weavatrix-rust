use super::{
    BTreeSet, EdgeKind, RepositoryState, Value, json, optional_u64, typed_client_relations,
    typed_diagnostics, typed_evidence, typed_identity, typed_key, typed_mismatch_kind, typed_nodes,
};

pub(in crate::operations) fn empty_typed_contracts(transport: &str) -> Value {
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

#[derive(Default)]
struct TypedEvaluation {
    contracts: Vec<Value>,
    matched_contracts: Vec<Value>,
    client_contracts: BTreeSet<(String, String)>,
    unmatched: usize,
}

/// Matches typed provider contracts to typed client operations/declarations.
///
/// GraphQL clients contribute parser-derived `Calls` edges. A protobuf file is
/// itself the gRPC wire contract, so a client-side service declaration is also
/// valid compatibility evidence; its exact RPC and streaming signature must
/// equal the provider declaration.
pub(in crate::operations) fn typed_api_contracts(
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

    let mut evaluation =
        match_provider_contracts(backend, clients, args, transport, &providers, per_item);
    let mismatches = find_client_mismatches(
        clients,
        args,
        transport,
        &providers,
        &provider_keys,
        &provider_identities,
        per_item,
        &mut evaluation.client_contracts,
    );

    ensure_typed_parsing_complete(backend, clients, transport)?;
    Ok(json!({
        "status": "COMPLETE",
        "transport": transport,
        "typedContractsV": 1,
        "totals": {
            "endpoints": providers.len(),
            "client_contracts": evaluation.client_contracts.len(),
            "matches": evaluation.matched_contracts.len(),
            "mismatches": mismatches.len(),
            "unmatched_endpoints": evaluation.unmatched,
            "parser_diagnostics": 0
        },
        "contracts": evaluation.contracts,
        "matches": evaluation.matched_contracts,
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

fn ensure_typed_parsing_complete(
    backend: &RepositoryState,
    clients: &[(String, &RepositoryState)],
    transport: &str,
) -> Result<(), String> {
    let mut diagnostics = typed_diagnostics(backend, "backend", transport);
    for (repository, client_state) in clients {
        diagnostics.extend(typed_diagnostics(client_state, repository, transport));
    }
    if diagnostics.is_empty() {
        Ok(())
    } else {
        Err(format!(
            "{transport} contract parsing failed closed: {}",
            diagnostics.join("; ")
        ))
    }
}

fn match_provider_contracts(
    backend: &RepositoryState,
    clients: &[(String, &RepositoryState)],
    args: &Value,
    transport: &str,
    providers: &[(usize, &weavatrix_graph::Node)],
    per_item: usize,
) -> TypedEvaluation {
    let mut evaluation = TypedEvaluation::default();
    for (_, endpoint) in providers {
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
                    evaluation
                        .client_contracts
                        .insert((repository.clone(), candidate.label.clone()));
                    client_evidence.extend(evidence);
                }
            }
        }
        client_evidence.truncate(per_item);
        let matched = !client_evidence.is_empty();
        evaluation.unmatched += usize::from(!matched);
        let contract = json!({
            "key": typed_key(&endpoint.label, transport),
            "transport": transport,
            "signature": endpoint.label,
            "backend": backend_evidence,
            "clients": client_evidence,
            "matched": matched
        });
        if matched {
            evaluation.matched_contracts.push(contract.clone());
        }
        evaluation.contracts.push(contract);
    }
    evaluation
}

#[allow(clippy::too_many_arguments)]
fn find_client_mismatches(
    clients: &[(String, &RepositoryState)],
    args: &Value,
    transport: &str,
    providers: &[(usize, &weavatrix_graph::Node)],
    provider_keys: &BTreeSet<String>,
    provider_identities: &BTreeSet<String>,
    per_item: usize,
    client_contracts: &mut BTreeSet<(String, String)>,
) -> Vec<Value> {
    // Same operation/RPC identity but a different operation kind or streaming
    // mode is stronger evidence than "not found": report the exact signature
    // disagreement instead of flattening it into an unmatched endpoint.
    let mut mismatches = Vec::new();
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
    mismatches
}
