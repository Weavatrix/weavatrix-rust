use super::{
    BTreeSet, Certainty, DEFAULT_MAX_FILE_BYTES, DEFAULT_MAX_FILES, DEFAULT_MAX_OBSERVATIONS,
    DEFAULT_PER_CONTRACT, Evaluation, MAX_MAX_FILE_BYTES, MAX_MAX_FILES, MAX_MAX_OBSERVATIONS,
    MAX_PER_CONTRACT, Observation, RUNTIME_SCHEMA, RepositoryState, RuntimeAggregate, ScanLimits,
    ScanSummary, Value, add_graph_fallbacks, bounded_u64, bounded_usize, evaluate, json,
    load_and_merge_runtime, observation_identity, scan_repository,
};

/// Produces richer event-transport contracts for `trace_api_contract`.
///
/// The pass only reads source files already present in each analyzed graph,
/// rejects paths that can escape the repository, never starts a process or
/// connects to a broker, and applies the shared production-first path filter.
pub(in crate::operations) fn event_contracts(
    backend: &RepositoryState,
    clients: &[(String, &RepositoryState)],
    args: &Value,
) -> Result<Value, String> {
    let max_files = bounded_usize(args, "max_source_files", DEFAULT_MAX_FILES, MAX_MAX_FILES);
    let max_file_bytes = bounded_u64(
        args,
        "max_source_file_bytes",
        DEFAULT_MAX_FILE_BYTES,
        MAX_MAX_FILE_BYTES,
    );
    let max_observations = bounded_usize(
        args,
        "max_matches",
        DEFAULT_MAX_OBSERVATIONS,
        MAX_MAX_OBSERVATIONS,
    );
    let max_contracts = bounded_usize(args, "max_endpoints", 250, 500);
    let per_contract = bounded_usize(
        args,
        "per_item_limit",
        DEFAULT_PER_CONTRACT,
        MAX_PER_CONTRACT,
    );

    let (mut observations, summary) = scan_repositories(
        backend,
        clients,
        args,
        max_files,
        max_file_bytes,
        max_observations,
    );
    let runtime = add_fallback_and_runtime_evidence(
        backend,
        clients,
        args,
        max_observations,
        &mut observations,
    );
    observations.sort();
    observations.dedup();
    observations.truncate(max_observations);

    let evaluated = evaluate(&observations, max_contracts, per_contract);
    let ambiguities = observations
        .iter()
        .filter(|observation| {
            (observation.resource.is_none() || observation.certainty == Certainty::Ambiguous)
                && !runtime.resolved_locations.contains(&(
                    observation.repository.clone(),
                    observation.path.clone(),
                    observation.line,
                ))
        })
        .take(per_contract)
        .map(Observation::to_json)
        .collect::<Vec<_>>();
    if summary.observation_limit_hit {
        return Err(
            "event transport analysis exceeded max_source_files or max_matches; raise the bound"
                .to_owned(),
        );
    }
    if summary.files_unreadable > 0 {
        return Err(format!(
            "event transport analysis could not read {} graph-listed source file(s)",
            summary.files_unreadable
        ));
    }
    if summary.files_skipped_oversize > 0 {
        return Err(format!(
            "{} event source file(s) exceeded max_source_file_bytes; raise the bound",
            summary.files_skipped_oversize
        ));
    }
    Ok(event_contract_response(
        &evaluated,
        observations.len(),
        &ambiguities,
        &summary,
        &runtime,
    ))
}

fn scan_repositories(
    backend: &RepositoryState,
    clients: &[(String, &RepositoryState)],
    args: &Value,
    max_files: usize,
    max_file_bytes: u64,
    max_observations: usize,
) -> (Vec<Observation>, ScanSummary) {
    let mut summary = ScanSummary::default();
    let mut observations = Vec::new();
    let limits = ScanLimits {
        files: max_files,
        file_bytes: max_file_bytes,
        observations: max_observations,
    };
    scan_repository(
        "backend",
        backend,
        args,
        limits,
        &mut observations,
        &mut summary,
    );
    let backend_source_observations = observations
        .iter()
        .filter(|observation| observation.origin == "tokenized_source")
        .cloned()
        .collect::<Vec<_>>();
    for (name, client_state) in clients {
        if observations.len() >= max_observations || summary.files_considered >= max_files {
            summary.observation_limit_hit = observations.len() >= max_observations;
            break;
        }
        if client_state.root() == backend.root()
            && client_state.snapshot().revision == backend.snapshot().revision
        {
            let remaining = max_observations.saturating_sub(observations.len());
            summary.observation_limit_hit = backend_source_observations.len() > remaining;
            observations.extend(
                backend_source_observations
                    .iter()
                    .take(remaining)
                    .cloned()
                    .map(|mut observation| {
                        observation.repository.clone_from(name);
                        observation
                    }),
            );
            continue;
        }
        scan_repository(
            name,
            client_state,
            args,
            limits,
            &mut observations,
            &mut summary,
        );
    }
    (observations, summary)
}

fn add_fallback_and_runtime_evidence(
    backend: &RepositoryState,
    clients: &[(String, &RepositoryState)],
    args: &Value,
    max_observations: usize,
    observations: &mut Vec<Observation>,
) -> RuntimeAggregate {
    observations.sort();
    observations.dedup();
    let source_keys = observations
        .iter()
        .filter(|item| item.origin == "tokenized_source")
        .filter_map(observation_identity)
        .collect::<BTreeSet<_>>();
    add_graph_fallbacks(
        "backend",
        backend,
        args,
        &source_keys,
        max_observations,
        observations,
    );
    for (name, client_state) in clients {
        add_graph_fallbacks(
            name,
            client_state,
            args,
            &source_keys,
            max_observations,
            observations,
        );
    }
    let mut runtime = RuntimeAggregate::default();
    load_and_merge_runtime("backend", backend, args, observations, &mut runtime);
    for (name, client_state) in clients {
        load_and_merge_runtime(name, client_state, args, observations, &mut runtime);
    }
    runtime
}

fn event_contract_response(
    evaluated: &Evaluation,
    observation_count: usize,
    ambiguities: &[Value],
    summary: &ScanSummary,
    runtime: &RuntimeAggregate,
) -> Value {
    json!({
        "status": "COMPLETE",
        "release_gate": if ambiguities.is_empty() {"PASS"} else {"BLOCKED_AMBIGUOUS_TRANSPORT_EVIDENCE"},
        "transport": "event",
        "transportContractsV": 3,
        "model_version": 2,
        "totals": {
            "contracts": evaluated.contracts.len(),
            "observations": observation_count,
            "matches": evaluated.matches.len(),
            "mismatches": evaluated.mismatches.len(),
            "ambiguities": evaluated.ambiguities.len() + ambiguities.len(),
            "files_considered": summary.files_considered,
            "files_scanned": summary.files_scanned,
            "files_without_transport_markers": summary.files_without_transport_markers,
            "files_without_transport_extractor": summary.files_without_transport_extractor,
            "files_unreadable": summary.files_unreadable,
            "files_skipped_oversize": summary.files_skipped_oversize
            ,"runtime_observations": runtime.observation_count
            ,"runtime_resolved": runtime.resolved_locations.len()
        },
        "contracts": evaluated.contracts,
        "matches": evaluated.matches,
        "mismatches": evaluated.mismatches,
        "ambiguities": evaluated.ambiguities,
        "ambiguous_evidence": ambiguities,
        "runtimeEvidence": {
            "status": runtime.status(),
            "present": runtime.present,
            "schema": RUNTIME_SCHEMA,
            "reports": runtime.reports,
            "resolvedDynamicLocations": runtime.resolved_locations.len(),
            "reasons": runtime.reasons,
            "absenceReasons": runtime.absence_reasons
        },
        "completeness": {
            "complete": true,
            "coverage": "COMPLETE",
            "reasons": []
        },
        "precision": "tokenized call-shape evidence plus graph-domain fallback",
        "semantics": {
            "kafka": "topic and producer/consumer role; consumer group is delivery metadata",
            "amqp": "direct queue matching plus exchange/binding/routing-key compatibility",
            "rabbitmq": "classified independently from generic AMQP while sharing AMQP routing compatibility",
            "jms": "topic/queue destination and producer/consumer role",
            "nats": "subject and publisher/subscriber role",
            "aws": "SQS queue and SNS topic are distinct; cross-service delivery requires explicit subscription evidence"
        },
        "production_first": true,
        "network": "NONE",
        "process": "NONE",
        "source_mutation": "NONE"
    })
}
