//! Bounded, offline event-transport contract matching.
//!
//! The graph carries useful generic domain nodes, but `topic == topic` is not
//! enough to prove that two repositories agree. Kafka topics, AMQP exchanges
//! and queues, JMS destinations, NATS subjects, and AWS SNS/SQS resources have
//! different delivery semantics. This module enriches graph evidence with a
//! tokenized source pass and keeps uncertainty explicit when a destination is
//! computed at runtime.

use crate::RepositoryState;
use blazingly_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Component, Path};
use weavatrix_graph::{AttributeValue, EdgeKind, NodeKind};
use weavatrix_parse::{Language, Token, TokenKind, tokenize};

const DEFAULT_MAX_FILES: usize = 5_000;
const MAX_MAX_FILES: usize = 25_000;
const DEFAULT_MAX_FILE_BYTES: u64 = 2 * 1_024 * 1_024;
const MAX_MAX_FILE_BYTES: u64 = 16 * 1_024 * 1_024;
const DEFAULT_MAX_OBSERVATIONS: usize = 5_000;
const MAX_MAX_OBSERVATIONS: usize = 20_000;
const DEFAULT_PER_CONTRACT: usize = 8;
const MAX_PER_CONTRACT: usize = 50;
const RUNTIME_SCHEMA: &str = "weavatrix.transport-runtime.v1";
const DEFAULT_RUNTIME_REPORTS: &[&str] = &[
    ".weavatrix/transport-runtime.json",
    ".weavatrix/reports/transport-runtime.json",
];
const MAX_RUNTIME_REPORT_BYTES: u64 = 2 * 1_024 * 1_024;
const MAX_RUNTIME_OBSERVATIONS: usize = 10_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Transport {
    Kafka,
    Amqp,
    RabbitMq,
    Jms,
    Nats,
    Sqs,
    Sns,
}

impl Transport {
    const fn as_str(self) -> &'static str {
        match self {
            Self::Kafka => "kafka",
            Self::Amqp => "amqp",
            Self::RabbitMq => "rabbitmq",
            Self::Jms => "jms",
            Self::Nats => "nats",
            Self::Sqs => "sqs",
            Self::Sns => "sns",
        }
    }

    const fn contract_family(self) -> Self {
        match self {
            Self::RabbitMq => Self::Amqp,
            concrete => concrete,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Entity {
    Topic,
    Queue,
    Exchange,
    Subject,
    Destination,
    Binding,
}

impl Entity {
    const fn as_str(self) -> &'static str {
        match self {
            Self::Topic => "topic",
            Self::Queue => "queue",
            Self::Exchange => "exchange",
            Self::Subject => "subject",
            Self::Destination => "destination",
            Self::Binding => "binding",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Role {
    Producer,
    Consumer,
    Declare,
    Bind,
}

impl Role {
    const fn as_str(self) -> &'static str {
        match self {
            Self::Producer => "producer",
            Self::Consumer => "consumer",
            Self::Declare => "declare",
            Self::Bind => "bind",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Certainty {
    Exact,
    Derived,
    Ambiguous,
}

impl Certainty {
    const fn as_str(self) -> &'static str {
        match self {
            Self::Exact => "exact",
            Self::Derived => "derived",
            Self::Ambiguous => "ambiguous",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct Observation {
    repository: String,
    path: String,
    line: u32,
    column: u32,
    language: String,
    transport: Transport,
    entity: Entity,
    role: Role,
    resource: Option<String>,
    exchange: Option<String>,
    routing_key: Option<String>,
    consumer_group: Option<String>,
    receiver: Option<String>,
    evidence: String,
    origin: &'static str,
    certainty: Certainty,
    uncertainty: Option<String>,
    candidates: BTreeSet<Transport>,
    runtime_observed: bool,
}

impl Observation {
    fn key(&self) -> Option<ContractKey> {
        Some(ContractKey {
            transport: self.transport.contract_family(),
            entity: self.entity,
            resource: self.resource.clone()?,
        })
    }

    fn to_json(&self) -> Value {
        json!({
            "repository": self.repository,
            "path": self.path,
            "span": {
                "start_line": self.line,
                "start_column": self.column,
                "end_line": self.line
            },
            "language": self.language,
            "transport": self.transport.as_str(),
            "entity": self.entity.as_str(),
            "role": self.role.as_str(),
            "resource": self.resource,
            "exchange": self.exchange,
            "routing_key": self.routing_key,
            "consumer_group": self.consumer_group,
            "receiver": self.receiver,
            "evidence": self.evidence,
            "origin": self.origin,
            "certainty": self.certainty.as_str(),
            "classification": {
                "selected": self.transport.as_str(),
                "candidates": self.candidates.iter().map(|candidate| candidate.as_str()).collect::<Vec<_>>(),
                "ambiguity": self.uncertainty
            },
            "runtime_observed": self.runtime_observed
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct ContractKey {
    transport: Transport,
    entity: Entity,
    resource: String,
}

#[derive(Debug, Default)]
struct ScanSummary {
    files_considered: usize,
    files_scanned: usize,
    files_without_transport_markers: usize,
    files_skipped_oversize: usize,
    files_unreadable: usize,
    files_without_transport_extractor: usize,
    observation_limit_hit: bool,
}

enum SourceScan {
    WithoutExtractor,
    Oversize,
    Unreadable,
    Observations(Vec<Observation>),
}

#[allow(clippy::struct_excessive_bools)]
#[derive(Debug, Clone, Copy, Default)]
struct ProviderHints {
    kafka: bool,
    amqp: bool,
    rabbitmq: bool,
    jms: bool,
    nats: bool,
    sqs: bool,
    sns: bool,
}

impl ProviderHints {
    fn from_bindings(bindings: &Bindings, tokens: &[Token], source: &str) -> Self {
        let mut hints = Self::default();
        for transport in bindings.transports.values().flatten() {
            match transport {
                Transport::Kafka => hints.kafka = true,
                Transport::Amqp => hints.amqp = true,
                Transport::RabbitMq => hints.rabbitmq = true,
                Transport::Jms => hints.jms = true,
                Transport::Nats => hints.nats = true,
                Transport::Sqs => hints.sqs = true,
                Transport::Sns => hints.sns = true,
            }
        }
        for token in tokens {
            if token.kind != TokenKind::Identifier {
                continue;
            }
            let text = token.text(source).to_ascii_lowercase();
            hints.kafka |= ["kafka", "kafkajs", "rdkafka", "sarama", "confluent_kafka"]
                .iter()
                .any(|needle| text.contains(needle));
            let rabbitmq = ["amqplib", "rabbitmq", "rabbit", "pika", "lapin", "kombu"]
                .iter()
                .any(|needle| text.contains(needle));
            hints.rabbitmq |= rabbitmq;
            hints.amqp |= text.contains("amqp") && !rabbitmq;
            hints.jms |= ["jms", "activemq", "artemis"]
                .iter()
                .any(|needle| text.contains(needle));
            hints.nats |= text.contains("nats");
            hints.sqs |= text.contains("sqs");
            hints.sns |= text.contains("sns");
        }
        hints
    }
}

#[derive(Debug, Default)]
struct Bindings {
    resources: BTreeMap<String, (Entity, String)>,
    consumer_groups: BTreeMap<String, String>,
    transports: BTreeMap<String, BTreeSet<Transport>>,
    aliases: BTreeMap<String, String>,
}

impl Bindings {
    fn from_tokens(tokens: &[Token], source: &str) -> Self {
        let mut bindings = Self::default();
        let mut lines = BTreeMap::<u32, Vec<&Token>>::new();
        for token in tokens {
            lines.entry(token.line).or_default().push(token);
        }
        for line in lines.values() {
            let joined = line
                .iter()
                .map(|token| token.text(source))
                .collect::<String>()
                .to_ascii_lowercase();
            let is_import = ["import", "from", "require", "use"]
                .iter()
                .any(|keyword| joined.contains(keyword));
            if !is_import {
                continue;
            }
            let providers = [
                Transport::Kafka,
                Transport::Amqp,
                Transport::RabbitMq,
                Transport::Jms,
                Transport::Nats,
                Transport::Sqs,
                Transport::Sns,
            ]
            .into_iter()
            .filter(|transport| provider_text_matches(*transport, &joined))
            .collect::<BTreeSet<_>>();
            if providers.is_empty() {
                continue;
            }
            for token in line
                .iter()
                .filter(|token| token.kind == TokenKind::Identifier)
            {
                let identifier = token.text(source);
                if !is_import_keyword(identifier) {
                    bindings
                        .transports
                        .entry(identifier.to_owned())
                        .or_default()
                        .extend(providers.iter().copied());
                }
            }
            for window in line.windows(3) {
                if window[0].kind == TokenKind::Identifier
                    && window[1].text(source).eq_ignore_ascii_case("as")
                    && window[2].kind == TokenKind::Identifier
                {
                    bindings.aliases.insert(
                        window[2].text(source).to_owned(),
                        window[0].text(source).to_owned(),
                    );
                }
            }
        }
        bindings
    }
}

#[derive(Debug)]
struct Call<'tokens, 'source> {
    name: String,
    chain: String,
    receiver: Option<String>,
    args: &'tokens [Token],
    line: u32,
    column: u32,
    evidence: String,
    source: &'source str,
}

#[derive(Debug)]
struct Detection {
    transport: Transport,
    entity: Entity,
    role: Role,
    resources: Vec<Option<String>>,
    exchange: Option<String>,
    routing_key: Option<String>,
    consumer_group: Option<String>,
    certainty: Certainty,
    uncertainty: Option<String>,
}

/// Produces richer event-transport contracts for `trace_api_contract`.
///
/// The pass only reads source files already present in each analyzed graph,
/// rejects paths that can escape the repository, never starts a process or
/// connects to a broker, and applies the shared production-first path filter.
#[allow(clippy::too_many_lines)]
pub(super) fn event_contracts(
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

    let mut summary = ScanSummary::default();
    let mut observations = Vec::new();
    scan_repository(
        "backend",
        backend,
        args,
        max_files,
        max_file_bytes,
        max_observations,
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
            max_files,
            max_file_bytes,
            max_observations,
            &mut observations,
            &mut summary,
        );
    }

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
        &mut observations,
    );
    for (name, client_state) in clients {
        add_graph_fallbacks(
            name,
            client_state,
            args,
            &source_keys,
            max_observations,
            &mut observations,
        );
    }
    let mut runtime = RuntimeAggregate::default();
    load_and_merge_runtime("backend", backend, args, &mut observations, &mut runtime);
    for (name, client_state) in clients {
        load_and_merge_runtime(name, client_state, args, &mut observations, &mut runtime);
    }
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
    Ok(json!({
        "status": "COMPLETE",
        "release_gate": if ambiguities.is_empty() {"PASS"} else {"BLOCKED_AMBIGUOUS_TRANSPORT_EVIDENCE"},
        "transport": "event",
        "transportContractsV": 3,
        "model_version": 2,
        "totals": {
            "contracts": evaluated.contracts.len(),
            "observations": observations.len(),
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
    }))
}

#[derive(Debug, Default)]
struct RuntimeAggregate {
    reports: Vec<Value>,
    reasons: Vec<String>,
    absence_reasons: Vec<String>,
    present: bool,
    observation_count: usize,
    resolved_locations: BTreeSet<(String, String, u32)>,
}

impl RuntimeAggregate {
    fn status(&self) -> &'static str {
        if !self.present {
            "NOT_PROVIDED"
        } else if self.reasons.is_empty() {
            "COMPLETE"
        } else {
            "REJECTED"
        }
    }
}

fn load_and_merge_runtime(
    repository: &str,
    state: &RepositoryState,
    args: &Value,
    observations: &mut Vec<Observation>,
    aggregate: &mut RuntimeAggregate,
) {
    let loaded = load_runtime_evidence(repository, state, args);
    aggregate.observation_count += loaded.observations.len();
    aggregate.present |= loaded.present;
    if loaded.present {
        aggregate.reasons.extend(loaded.reasons.iter().cloned());
    } else {
        aggregate
            .absence_reasons
            .extend(loaded.reasons.iter().cloned());
    }
    for runtime in loaded.observations {
        if !runtime.path.is_empty() && runtime.line > 0 {
            aggregate.resolved_locations.insert((
                repository.to_owned(),
                runtime.path.clone(),
                runtime.line,
            ));
        }
        if let Some(existing) = observations.iter_mut().find(|existing| {
            existing.repository == runtime.repository
                && existing.transport == runtime.transport
                && existing.entity == runtime.entity
                && existing.role == runtime.role
                && existing.resource == runtime.resource
        }) {
            existing.runtime_observed = true;
        } else {
            observations.push(runtime);
        }
    }
    aggregate.reports.push(json!({
        "repository": repository,
        "status": loaded.status,
        "present": loaded.present,
        "file": loaded.file,
        "generatedAt": loaded.generated_at,
        "repositoryRevision": loaded.repository_revision,
        "coverage": loaded.coverage,
        "observationCount": loaded.observation_count,
        "reasons": loaded.reasons
    }));
}

struct RuntimeLoad {
    status: &'static str,
    present: bool,
    file: Option<String>,
    generated_at: Option<String>,
    repository_revision: Option<String>,
    coverage: Value,
    observations: Vec<Observation>,
    observation_count: usize,
    reasons: Vec<String>,
}

fn load_runtime_evidence(repository: &str, state: &RepositoryState, args: &Value) -> RuntimeLoad {
    let explicit = args
        .get("runtime_evidence_files")
        .and_then(|files| {
            files
                .get(repository)
                .or_else(|| files.get(state.root().to_string_lossy().as_ref()))
        })
        .and_then(Value::as_str)
        .map(str::to_owned);
    let candidates = explicit
        .as_deref()
        .map_or_else(|| DEFAULT_RUNTIME_REPORTS.to_vec(), |file| vec![file]);
    for candidate in candidates {
        let Some(path) = safe_repository_path(state.root(), candidate) else {
            if explicit.is_some() {
                return runtime_error(
                    Some(candidate.to_owned()),
                    format!("{repository}: runtime evidence path escapes the repository"),
                );
            }
            continue;
        };
        let Ok(metadata) = fs::metadata(&path) else {
            if explicit.is_some() {
                return runtime_error(
                    Some(candidate.to_owned()),
                    format!("{repository}: runtime evidence file is unreadable"),
                );
            }
            continue;
        };
        if metadata.len() > MAX_RUNTIME_REPORT_BYTES {
            return runtime_error(
                Some(candidate.to_owned()),
                format!("{repository}: runtime evidence exceeds {MAX_RUNTIME_REPORT_BYTES} bytes"),
            );
        }
        let Ok(bytes) = fs::read(&path) else {
            return runtime_error(
                Some(candidate.to_owned()),
                format!("{repository}: runtime evidence file is unreadable"),
            );
        };
        let Ok(report) = blazingly_json::from_slice::<Value>(&bytes) else {
            return runtime_error(
                Some(candidate.to_owned()),
                format!("{repository}: runtime evidence is invalid JSON"),
            );
        };
        return validate_runtime_report(repository, state, args, candidate, &report);
    }
    runtime_error(
        None,
        format!("{repository}: no revision-bound runtime transport evidence was found"),
    )
}

fn runtime_error(file: Option<String>, reason: String) -> RuntimeLoad {
    let present = file.is_some();
    RuntimeLoad {
        status: if present { "REJECTED" } else { "NOT_PROVIDED" },
        present,
        file,
        generated_at: None,
        repository_revision: None,
        coverage: json!({}),
        observations: Vec::new(),
        observation_count: 0,
        reasons: vec![reason],
    }
}

fn validate_runtime_report(
    repository: &str,
    state: &RepositoryState,
    args: &Value,
    file: &str,
    report: &Value,
) -> RuntimeLoad {
    let mut reasons = Vec::new();
    let mut usable = true;
    if report["schema"].as_str() != Some(RUNTIME_SCHEMA) {
        reasons.push(format!(
            "{repository}: runtime evidence schema is not recognized"
        ));
        usable = false;
    }
    let revision = report["repositoryRevision"].as_str().map(str::to_owned);
    if revision.as_deref() != Some(state.snapshot().revision.as_str()) {
        reasons.push(format!(
            "{repository}: runtime evidence revision does not match the active graph"
        ));
        usable = false;
    }
    let generated_at = report["generatedAt"].as_str().map(str::to_owned);
    let generated_millis = report.get("generatedAt").and_then(timestamp_millis);
    let now_millis = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .ok()
        .and_then(|duration| u64::try_from(duration.as_millis()).ok())
        .unwrap_or(0);
    let max_age_hours = args
        .get("runtime_evidence_max_age_hours")
        .and_then(Value::as_u64)
        .unwrap_or(168)
        .clamp(1, 8_760);
    let max_age_millis = max_age_hours.saturating_mul(3_600_000);
    match generated_millis {
        None => {
            reasons.push(format!(
                "{repository}: runtime evidence generatedAt is invalid"
            ));
            usable = false;
        }
        Some(generated)
            if generated > now_millis.saturating_add(300_000)
                || now_millis.saturating_sub(generated) > max_age_millis =>
        {
            reasons.push(format!(
                "{repository}: runtime evidence is stale or from the future"
            ));
            usable = false;
        }
        Some(_) => {}
    }
    let coverage_status = report
        .pointer("/coverage/event")
        .and_then(Value::as_str)
        .unwrap_or("NOT_CHECKED")
        .to_ascii_uppercase();
    if coverage_status != "COMPLETE" {
        reasons.push(format!(
            "{repository}: event runtime capture is {coverage_status}"
        ));
    }
    let coverage = json!({"event": coverage_status});
    let raw = report["observations"]
        .as_array()
        .into_iter()
        .flatten()
        .cloned()
        .chain(otlp_event_observations(report))
        .take(MAX_RUNTIME_OBSERVATIONS)
        .collect::<Vec<_>>();
    let observations = if usable {
        raw.iter()
            .filter_map(|item| normalize_runtime_observation(repository, state, item))
            .collect::<Vec<_>>()
    } else {
        Vec::new()
    };
    if usable && raw.len() != observations.len() {
        reasons.push(format!(
            "{repository}: {} invalid runtime event observation(s) were ignored",
            raw.len().saturating_sub(observations.len())
        ));
    }
    RuntimeLoad {
        status: if reasons.is_empty() {
            "COMPLETE"
        } else {
            "REJECTED"
        },
        present: true,
        file: Some(file.to_owned()),
        generated_at,
        repository_revision: revision,
        coverage,
        observation_count: observations.len(),
        observations,
        reasons,
    }
}

fn normalize_runtime_observation(
    repository: &str,
    state: &RepositoryState,
    raw: &Value,
) -> Option<Observation> {
    let declared_transport = raw["transport"]
        .as_str()
        .unwrap_or_default()
        .to_ascii_lowercase();
    if !matches!(
        declared_transport.as_str(),
        "event" | "kafka" | "amqp" | "rabbitmq" | "jms" | "nats" | "sqs" | "sns"
    ) {
        return None;
    }
    let system = raw
        .get("system")
        .and_then(Value::as_str)
        .unwrap_or(declared_transport.as_str())
        .to_ascii_lowercase();
    let transport = runtime_transport(&system)?;
    let side = raw["side"].as_str()?.to_ascii_lowercase();
    let role = match side.as_str() {
        "publisher" | "producer" => Role::Producer,
        "subscriber" | "consumer" => Role::Consumer,
        _ => return None,
    };
    let resource = bounded_text(raw.get("name")?, 1_024)?;
    let kind = raw
        .get("destinationKind")
        .or_else(|| raw.get("kind"))
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_ascii_lowercase();
    let entity = runtime_entity(transport, &kind, role);
    let path = raw
        .get("file")
        .and_then(Value::as_str)
        .and_then(|file| safe_source_path(state.root(), file))
        .unwrap_or_default();
    let line = raw
        .get("line")
        .and_then(Value::as_u64)
        .and_then(|line| u32::try_from(line).ok())
        .filter(|line| *line > 0)
        .unwrap_or(0);
    Some(Observation {
        repository: repository.to_owned(),
        path,
        line,
        column: 0,
        language: "runtime".to_owned(),
        transport,
        entity,
        role,
        resource: Some(resource),
        exchange: raw
            .get("exchange")
            .and_then(|value| bounded_text(value, 1_024)),
        routing_key: raw
            .get("routingKey")
            .or_else(|| raw.get("routing_key"))
            .and_then(|value| bounded_text(value, 1_024)),
        consumer_group: raw
            .get("consumerGroup")
            .or_else(|| raw.get("consumer_group"))
            .and_then(|value| bounded_text(value, 1_024)),
        receiver: None,
        evidence: raw
            .get("detector")
            .and_then(Value::as_str)
            .unwrap_or("runtime-report")
            .to_owned(),
        origin: "runtime_evidence",
        certainty: Certainty::Exact,
        uncertainty: None,
        candidates: BTreeSet::from([transport]),
        runtime_observed: true,
    })
}

fn runtime_transport(system: &str) -> Option<Transport> {
    [
        Transport::Kafka,
        Transport::Amqp,
        Transport::RabbitMq,
        Transport::Jms,
        Transport::Nats,
        Transport::Sqs,
        Transport::Sns,
    ]
    .into_iter()
    .find(|transport| provider_text_matches(*transport, system))
}

fn runtime_entity(transport: Transport, kind: &str, role: Role) -> Entity {
    if kind.contains("exchange") {
        return Entity::Exchange;
    }
    if kind.contains("queue") {
        return Entity::Queue;
    }
    match transport {
        Transport::Kafka | Transport::Sns => Entity::Topic,
        Transport::Amqp | Transport::RabbitMq if role == Role::Producer => Entity::Exchange,
        Transport::Amqp | Transport::RabbitMq | Transport::Sqs => Entity::Queue,
        Transport::Nats => Entity::Subject,
        Transport::Jms => Entity::Destination,
    }
}

fn bounded_text(value: &Value, max: usize) -> Option<String> {
    let text = value.as_str()?.trim();
    (!text.is_empty()).then(|| text.chars().take(max).collect())
}

fn safe_repository_path(root: &Path, candidate: &str) -> Option<std::path::PathBuf> {
    let candidate = Path::new(candidate);
    if candidate.is_absolute() {
        let root = root.canonicalize().ok()?;
        let path = candidate.canonicalize().ok()?;
        return path.starts_with(&root).then_some(path);
    }
    if candidate.components().any(|component| {
        matches!(
            component,
            Component::ParentDir | Component::RootDir | Component::Prefix(_)
        )
    }) {
        return None;
    }
    Some(root.join(candidate))
}

fn safe_source_path(root: &Path, candidate: &str) -> Option<String> {
    if candidate.contains('\0') {
        return None;
    }
    let normalized = candidate.replace('\\', "/");
    let path = Path::new(&normalized);
    if !path.is_absolute() {
        if path.components().any(|component| {
            matches!(
                component,
                Component::ParentDir | Component::RootDir | Component::Prefix(_)
            )
        }) {
            return None;
        }
        return Some(normalized.trim_start_matches("./").to_owned());
    }
    let root = root.canonicalize().ok()?;
    let path = path.canonicalize().ok()?;
    path.strip_prefix(root)
        .ok()
        .map(|relative| relative.to_string_lossy().replace('\\', "/"))
}

fn otlp_event_observations(report: &Value) -> impl Iterator<Item = Value> + '_ {
    let root = report.get("otlp").unwrap_or(report);
    root["resourceSpans"]
        .as_array()
        .into_iter()
        .flatten()
        .flat_map(|resource| {
            resource["scopeSpans"]
                .as_array()
                .or_else(|| resource["instrumentationLibrarySpans"].as_array())
                .into_iter()
                .flatten()
        })
        .flat_map(|scope| scope["spans"].as_array().into_iter().flatten())
        .filter_map(otlp_event_observation)
}

fn otlp_event_observation(span: &Value) -> Option<Value> {
    let attributes = otlp_attributes(&span["attributes"]);
    let system = attributes.get("messaging.system")?.to_ascii_lowercase();
    let name = attributes
        .get("messaging.destination.name")
        .or_else(|| attributes.get("messaging.destination"))?
        .clone();
    let side = otlp_span_side(span)?;
    Some(json_object_observation(&attributes, &system, &name, side))
}

fn json_object_observation(
    attributes: &BTreeMap<String, String>,
    system: &str,
    name: &str,
    side: &'static str,
) -> Value {
    let line = attributes
        .get("code.line.number")
        .or_else(|| attributes.get("code.lineno"))
        .and_then(|line| line.parse::<u64>().ok());
    json!({
        "transport": "event",
        "system": system,
        "side": side,
        "name": name,
        "kind": attributes.get("messaging.operation.type")
            .or_else(|| attributes.get("messaging.operation")),
        "file": attributes.get("code.file.path")
            .or_else(|| attributes.get("code.filepath")),
        "line": line,
        "consumerGroup": attributes.get("messaging.kafka.consumer.group"),
        "routingKey": attributes.get("messaging.rabbitmq.destination.routing_key"),
        "detector": "otlp-span"
    })
}

fn otlp_attributes(value: &Value) -> BTreeMap<String, String> {
    value
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|attribute| {
            let key = attribute["key"].as_str()?.to_owned();
            let value = attribute
                .pointer("/value/stringValue")
                .and_then(Value::as_str)
                .map(str::to_owned)
                .or_else(|| {
                    attribute
                        .pointer("/value/intValue")
                        .and_then(Value::as_str)
                        .map(str::to_owned)
                })
                .or_else(|| attribute["value"].as_str().map(str::to_owned))?;
            Some((key, value))
        })
        .collect()
}

fn otlp_span_side(span: &Value) -> Option<&'static str> {
    if let Some(kind) = span["kind"].as_u64() {
        return match kind {
            4 => Some("publisher"),
            5 => Some("subscriber"),
            _ => None,
        };
    }
    let kind = span["kind"].as_str()?.to_ascii_uppercase();
    if kind.contains("PRODUCER") {
        Some("publisher")
    } else if kind.contains("CONSUMER") {
        Some("subscriber")
    } else {
        None
    }
}

fn timestamp_millis(value: &Value) -> Option<u64> {
    if let Some(value) = value.as_u64() {
        return Some(if value < 10_000_000_000 {
            value.saturating_mul(1_000)
        } else {
            value
        });
    }
    parse_rfc3339_millis(value.as_str()?)
}

fn parse_rfc3339_millis(value: &str) -> Option<u64> {
    let bytes = value.as_bytes();
    if bytes.len() < 20
        || bytes.get(4) != Some(&b'-')
        || bytes.get(7) != Some(&b'-')
        || !matches!(bytes.get(10), Some(b'T' | b't' | b' '))
        || bytes.get(13) != Some(&b':')
        || bytes.get(16) != Some(&b':')
    {
        return None;
    }
    let year = i64::from(decimal(bytes, 0, 4)?);
    let month = i64::from(decimal(bytes, 5, 7)?);
    let day = i64::from(decimal(bytes, 8, 10)?);
    let hour = i64::from(decimal(bytes, 11, 13)?);
    let minute = i64::from(decimal(bytes, 14, 16)?);
    let second = i64::from(decimal(bytes, 17, 19)?);
    if !(1..=12).contains(&month)
        || !(1..=31).contains(&day)
        || hour > 23
        || minute > 59
        || second > 60
    {
        return None;
    }
    let mut cursor = 19_usize;
    let mut millis = 0_i64;
    if bytes.get(cursor) == Some(&b'.') {
        cursor += 1;
        let start = cursor;
        while bytes.get(cursor).is_some_and(u8::is_ascii_digit) {
            cursor += 1;
        }
        let fraction = &value[start..cursor];
        let padded = format!("{fraction:0<3}");
        millis = padded.get(..3)?.parse().ok()?;
    }
    let offset_seconds = match bytes.get(cursor) {
        Some(b'Z' | b'z') if cursor + 1 == bytes.len() => 0_i64,
        Some(sign @ (b'+' | b'-')) if cursor + 6 == bytes.len() => {
            let offset_hour = i64::from(decimal(bytes, cursor + 1, cursor + 3)?);
            let offset_minute = i64::from(decimal(bytes, cursor + 4, cursor + 6)?);
            if bytes.get(cursor + 3) != Some(&b':') || offset_hour > 23 || offset_minute > 59 {
                return None;
            }
            let offset = offset_hour * 3_600 + offset_minute * 60;
            if *sign == b'+' { offset } else { -offset }
        }
        _ => return None,
    };
    let days = days_from_civil(year, month, day)?;
    let seconds = days
        .checked_mul(86_400)?
        .checked_add(hour * 3_600 + minute * 60 + second)?
        .checked_sub(offset_seconds)?;
    u64::try_from(seconds.checked_mul(1_000)?.checked_add(millis)?).ok()
}

fn decimal(bytes: &[u8], start: usize, end: usize) -> Option<u32> {
    bytes
        .get(start..end)?
        .iter()
        .try_fold(0_u32, |value, byte| {
            byte.is_ascii_digit()
                .then(|| value * 10 + u32::from(*byte - b'0'))
        })
}

fn days_from_civil(year: i64, month: i64, day: i64) -> Option<i64> {
    let leap = |year: i64| year % 4 == 0 && (year % 100 != 0 || year % 400 == 0);
    let max_day = match month {
        2 if leap(year) => 29,
        2 => 28,
        4 | 6 | 9 | 11 => 30,
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        _ => return None,
    };
    if !(1..=max_day).contains(&day) {
        return None;
    }
    let adjusted_year = year - i64::from(month <= 2);
    let era = if adjusted_year >= 0 {
        adjusted_year
    } else {
        adjusted_year - 399
    } / 400;
    let year_of_era = adjusted_year - era * 400;
    let adjusted_month = month + if month > 2 { -3 } else { 9 };
    let day_of_year = (153 * adjusted_month + 2) / 5 + day - 1;
    let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;
    Some(era * 146_097 + day_of_era - 719_468)
}

struct Evaluation {
    contracts: Vec<Value>,
    matches: Vec<Value>,
    mismatches: Vec<Value>,
    ambiguities: Vec<Value>,
}

#[allow(clippy::too_many_lines)]
fn evaluate(observations: &[Observation], max_contracts: usize, per_contract: usize) -> Evaluation {
    let mut grouped = BTreeMap::<ContractKey, Vec<&Observation>>::new();
    for observation in observations {
        if observation.certainty == Certainty::Ambiguous {
            continue;
        }
        if let Some(key) = observation.key() {
            grouped.entry(key).or_default().push(observation);
        }
    }
    let bindings = observations
        .iter()
        .filter(|observation| {
            matches!(observation.transport, Transport::Amqp | Transport::RabbitMq)
        })
        .filter(|observation| observation.role == Role::Bind)
        .collect::<Vec<_>>();
    let mut contracts = Vec::new();
    let mut matches = Vec::new();
    let mut mismatches = Vec::new();
    let mut ambiguities = Vec::new();

    for (key, evidence) in grouped.into_iter().take(max_contracts) {
        let producers = evidence
            .iter()
            .copied()
            .filter(|item| item.role == Role::Producer)
            .collect::<Vec<_>>();
        let mut consumers = evidence
            .iter()
            .copied()
            .filter(|item| item.role == Role::Consumer)
            .collect::<Vec<_>>();
        let declarations = evidence
            .iter()
            .copied()
            .filter(|item| matches!(item.role, Role::Declare | Role::Bind))
            .collect::<Vec<_>>();

        let mut routed = Vec::new();
        if key.transport == Transport::Amqp && key.entity == Entity::Exchange {
            for producer in &producers {
                for binding in &bindings {
                    if binding.exchange.as_deref() != Some(key.resource.as_str()) {
                        continue;
                    }
                    let compatible = routing_keys_match(
                        producer.routing_key.as_deref(),
                        binding.routing_key.as_deref(),
                    );
                    let queue = binding.resource.as_deref().unwrap_or_default();
                    for candidate in observations.iter().filter(|candidate| {
                        matches!(candidate.transport, Transport::Amqp | Transport::RabbitMq)
                            && candidate.entity == Entity::Queue
                            && candidate.role == Role::Consumer
                            && candidate.resource.as_deref() == Some(queue)
                    }) {
                        if compatible {
                            consumers.push(candidate);
                            routed.push(json!({
                                "producer": producer.to_json(),
                                "binding": binding.to_json(),
                                "consumer": candidate.to_json(),
                                "routing": "COMPATIBLE"
                            }));
                        } else {
                            mismatches.push(json!({
                                "code": "AMQP_ROUTING_KEY_MISMATCH",
                                "exchange": key.resource,
                                "producer": producer.to_json(),
                                "binding": binding.to_json(),
                                "consumer": candidate.to_json()
                            }));
                        }
                    }
                }
            }
        }
        consumers.sort();
        consumers.dedup();
        let providers = evidence
            .iter()
            .map(|item| item.transport)
            .collect::<BTreeSet<_>>();
        let provider = if providers.len() == 1 {
            providers.first().copied().unwrap_or(key.transport).as_str()
        } else {
            key.transport.as_str()
        };
        let paired = !producers.is_empty() && !consumers.is_empty();
        let verdict = if paired {
            "MATCH"
        } else if producers.is_empty() && consumers.is_empty() {
            "CONFIGURATION_ONLY"
        } else {
            "UNPAIRED_STATIC_EVIDENCE"
        };
        let key_text = format!(
            "{}:{}:{}",
            key.transport.as_str(),
            key.entity.as_str(),
            key.resource
        );
        if paired {
            matches.push(json!({
                "key": key_text,
                "producer_count": producers.len(),
                "consumer_count": consumers.len(),
                "routed_matches": routed
            }));
        } else if !producers.is_empty() || !consumers.is_empty() {
            ambiguities.push(json!({
                "code": "UNPAIRED_STATIC_EVIDENCE",
                "key": key_text,
                "candidates": ["external_peer", "runtime_configuration", "repository_outside_scope"],
                "evidence": evidence.iter().take(per_contract).map(|item| item.to_json()).collect::<Vec<_>>()
            }));
        }
        contracts.push(json!({
            "key": key_text,
            "transport": "event",
            "provider": provider,
            "contract_family": key.transport.as_str(),
            "provider_candidates": providers.iter().map(|provider| provider.as_str()).collect::<Vec<_>>(),
            "entity": key.entity.as_str(),
            "kind": key.entity.as_str(),
            "name": key.resource,
            "verdict": verdict,
            "matched": paired,
            "backend_contract": evidence.iter()
                .find(|item| item.repository == "backend")
                .map(|item| item.to_json()),
            "callsites": evidence.iter()
                .filter(|item| item.repository != "backend")
                .take(per_contract)
                .map(|item| item.to_json())
                .collect::<Vec<_>>(),
            "producers": producers.into_iter().take(per_contract).map(Observation::to_json).collect::<Vec<_>>(),
            "consumers": consumers.into_iter().take(per_contract).map(Observation::to_json).collect::<Vec<_>>(),
            "configuration": declarations.into_iter().take(per_contract).map(Observation::to_json).collect::<Vec<_>>()
        }));
    }

    add_cross_kind_mismatches(observations, &mut mismatches);
    Evaluation {
        contracts,
        matches,
        mismatches,
        ambiguities,
    }
}

fn add_cross_kind_mismatches(observations: &[Observation], mismatches: &mut Vec<Value>) {
    let producers = observations
        .iter()
        .filter(|item| item.role == Role::Producer)
        .filter_map(|item| item.resource.as_deref().map(|resource| (item, resource)));
    for (producer, resource) in producers {
        for consumer in observations.iter().filter(|item| {
            item.role == Role::Consumer && item.resource.as_deref() == Some(resource)
        }) {
            if producer.transport.contract_family() != consumer.transport.contract_family() {
                mismatches.push(json!({
                    "code": "TRANSPORT_MISMATCH",
                    "resource": resource,
                    "producer": producer.to_json(),
                    "consumer": consumer.to_json()
                }));
            } else if producer.transport.contract_family() == consumer.transport.contract_family()
                && producer.entity != consumer.entity
                && !matches!(
                    (
                        producer.transport.contract_family(),
                        producer.entity,
                        consumer.entity
                    ),
                    (Transport::Jms, Entity::Destination, _)
                        | (Transport::Jms, _, Entity::Destination)
                )
            {
                mismatches.push(json!({
                    "code": "DESTINATION_KIND_MISMATCH",
                    "resource": resource,
                    "producer": producer.to_json(),
                    "consumer": consumer.to_json()
                }));
            }
        }
    }
    mismatches.sort_by(|left, right| {
        left["code"]
            .as_str()
            .unwrap_or_default()
            .cmp(right["code"].as_str().unwrap_or_default())
            .then_with(|| {
                left["resource"]
                    .as_str()
                    .unwrap_or_default()
                    .cmp(right["resource"].as_str().unwrap_or_default())
            })
    });
    mismatches.dedup();
}

fn routing_keys_match(producer: Option<&str>, binding: Option<&str>) -> bool {
    let Some(producer) = producer else {
        return false;
    };
    let Some(binding) = binding else {
        return false;
    };
    if producer == binding {
        return true;
    }
    let producer = producer.split('.').collect::<Vec<_>>();
    let binding = binding.split('.').collect::<Vec<_>>();
    let mut left = 0_usize;
    let mut right = 0_usize;
    while left < producer.len() && right < binding.len() {
        match binding[right] {
            "#" => return true,
            "*" => {
                left += 1;
                right += 1;
            }
            exact if exact == producer[left] => {
                left += 1;
                right += 1;
            }
            _ => return false,
        }
    }
    (left == producer.len() && right == binding.len())
        || (right + 1 == binding.len() && binding[right] == "#")
}

#[allow(clippy::too_many_arguments)]
fn scan_repository(
    repository: &str,
    state: &RepositoryState,
    args: &Value,
    max_files: usize,
    max_file_bytes: u64,
    max_observations: usize,
    observations: &mut Vec<Observation>,
    summary: &mut ScanSummary,
) {
    if observations.len() >= max_observations || summary.files_considered >= max_files {
        return;
    }
    let files = state
        .graph()
        .nodes()
        .iter()
        .filter(|node| node.kind == NodeKind::File)
        .filter(|node| super::health::path_is_visible(&node.label, args))
        .map(|node| {
            let candidate = !matches!(
                node.attributes.get("transport_candidate"),
                Some(AttributeValue::Bool(false))
            );
            (node.label.as_str(), candidate)
        })
        .collect::<BTreeMap<_, _>>()
        .into_iter()
        .take(max_files.saturating_sub(summary.files_considered))
        .collect::<Vec<_>>();
    summary.files_considered += files.len();
    for (path, candidate) in files {
        if !candidate {
            summary.files_without_transport_markers += 1;
            continue;
        }
        if observations.len() >= max_observations {
            summary.observation_limit_hit = true;
            break;
        }
        let outcome = scan_source(state.root(), repository, path, max_file_bytes);
        match outcome {
            SourceScan::WithoutExtractor => summary.files_without_transport_extractor += 1,
            SourceScan::Oversize => summary.files_skipped_oversize += 1,
            SourceScan::Unreadable => summary.files_unreadable += 1,
            SourceScan::Observations(found) => {
                summary.files_scanned += 1;
                let remaining = max_observations.saturating_sub(observations.len());
                summary.observation_limit_hit |= found.len() > remaining;
                observations.extend(found.into_iter().take(remaining));
            }
        }
    }
}

fn scan_source(root: &Path, repository: &str, path: &str, max_file_bytes: u64) -> SourceScan {
    let Some(language) = language_for_path(path) else {
        return SourceScan::WithoutExtractor;
    };
    let relative = Path::new(path);
    if relative.is_absolute()
        || relative.components().any(|component| {
            matches!(
                component,
                Component::ParentDir | Component::RootDir | Component::Prefix(_)
            )
        })
    {
        return SourceScan::Unreadable;
    }
    let path_on_disk = root.join(relative);
    let Ok(metadata) = fs::metadata(&path_on_disk) else {
        return SourceScan::Unreadable;
    };
    if metadata.len() > max_file_bytes {
        return SourceScan::Oversize;
    }
    let Ok(source) = fs::read_to_string(path_on_disk) else {
        return SourceScan::Unreadable;
    };
    SourceScan::Observations(extract_observations(repository, path, language, &source))
}

#[cfg(test)]
fn may_contain_transport_observation(source: &str) -> bool {
    crate::language::may_contain_transport_marker(source)
}

fn language_for_path(path: &str) -> Option<Language> {
    let extension = path.rsplit_once('.')?.1;
    Language::from_extension(extension)
}

#[allow(clippy::too_many_lines)]
fn extract_observations(
    repository: &str,
    path: &str,
    language: Language,
    source: &str,
) -> Vec<Observation> {
    // Start from the lossless stream. Dropping trivia here is a derived view;
    // the tokenizer, spans, and evidence still refer to the original bytes.
    let tokens = tokenize(source, language)
        .into_iter()
        .filter(|token| !token.is_trivia())
        .collect::<Vec<_>>();
    let mut bindings = Bindings::from_tokens(&tokens, source);
    let hints = ProviderHints::from_bindings(&bindings, &tokens, source);
    let mut observations = Vec::new();
    let mut index = 0_usize;
    while index < tokens.len() {
        if tokens[index].text(source) == "=" {
            propagate_assignment_binding(&tokens, source, index, &mut bindings);
        }
        if tokens[index].text(source) != "(" {
            index += 1;
            continue;
        }
        let Some(name_index) = call_name_index(&tokens, source, index) else {
            index += 1;
            continue;
        };
        let Some(close) = matching_close(&tokens, source, index) else {
            break;
        };
        let chain = call_chain(&tokens, source, name_index);
        let name = tokens[name_index].text(source).to_owned();
        let receiver = receiver_name(&tokens, source, name_index);
        let call = Call {
            name,
            chain,
            receiver,
            args: &tokens[index + 1..close],
            line: tokens[name_index].line,
            column: tokens[name_index].column,
            evidence: source_line(source, tokens[name_index].line),
            source,
        };
        let detections = detect(&call, hints, &bindings);
        let ambiguity_candidates = detections
            .iter()
            .filter(|detection| detection.certainty == Certainty::Ambiguous)
            .map(|detection| detection.transport)
            .collect::<BTreeSet<_>>();
        remember_transport_origin(
            &tokens,
            source,
            name_index,
            &call,
            &detections,
            &mut bindings,
        );
        for detection in detections {
            remember_binding(&tokens, source, name_index, &detection, &mut bindings);
            for resource in detection.resources {
                let computed_destination = resource.is_none();
                let certainty = if computed_destination {
                    Certainty::Ambiguous
                } else {
                    detection.certainty
                };
                let candidates = if detection.certainty == Certainty::Ambiguous {
                    ambiguity_candidates.clone()
                } else {
                    BTreeSet::from([detection.transport])
                };
                observations.push(Observation {
                    repository: repository.to_owned(),
                    path: path.to_owned(),
                    line: call.line,
                    column: call.column,
                    language: language.as_str().to_owned(),
                    transport: detection.transport,
                    entity: detection.entity,
                    role: detection.role,
                    resource,
                    exchange: detection.exchange.clone(),
                    routing_key: detection.routing_key.clone(),
                    consumer_group: detection.consumer_group.clone().or_else(|| {
                        call.receiver
                            .as_ref()
                            .and_then(|receiver| bindings.consumer_groups.get(receiver))
                            .cloned()
                    }),
                    receiver: call.receiver.clone(),
                    evidence: call.evidence.clone(),
                    origin: "tokenized_source",
                    certainty,
                    uncertainty: if computed_destination {
                        Some(
                            "destination is computed; the exact transport is proven but the resource name requires runtime evidence"
                                .to_owned(),
                        )
                    } else {
                        detection.uncertainty.clone()
                    },
                    candidates,
                    runtime_observed: false,
                });
            }
        }
        index += 1;
    }
    observations
}

fn propagate_assignment_binding(
    tokens: &[Token],
    source: &str,
    equals: usize,
    bindings: &mut Bindings,
) {
    let Some(variable) = (equals.saturating_sub(4)..equals).rev().find_map(|index| {
        (tokens[index].kind == TokenKind::Identifier).then(|| tokens[index].text(source).to_owned())
    }) else {
        return;
    };
    let line = tokens[equals].line;
    let transports = tokens
        .iter()
        .skip(equals + 1)
        .take_while(|token| token.line == line && token.text(source) != ";")
        .filter(|token| token.kind == TokenKind::Identifier)
        .filter_map(|token| bindings.transports.get(token.text(source)))
        .flatten()
        .copied()
        .collect::<BTreeSet<_>>();
    if !transports.is_empty() {
        bindings
            .transports
            .entry(variable)
            .or_default()
            .extend(transports);
    }
}

fn detect(call: &Call<'_, '_>, hints: ProviderHints, bindings: &Bindings) -> Vec<Detection> {
    let semantic_name = bindings
        .aliases
        .get(&call.name)
        .map_or(call.name.as_str(), String::as_str);
    let name = semantic_name.to_ascii_lowercase();
    let chain = call.chain.to_ascii_lowercase();
    let candidates = call_transport_candidates(call, &name, &chain, hints, bindings);
    if candidates.is_empty() {
        return detect_with_hints(call, &name, &chain, hints, bindings)
            .into_iter()
            .collect();
    }
    let union = candidates.len() > 1;
    candidates
        .into_iter()
        .filter_map(|transport| {
            let selected = hints_for(transport);
            let mut detection = detect_with_hints(call, &name, &chain, selected, bindings)?;
            if union {
                detection.certainty = Certainty::Ambiguous;
                detection.uncertainty = Some(
                    "multiple concrete transports are reachable from receiver/import provenance"
                        .to_owned(),
                );
            }
            Some(detection)
        })
        .collect()
}

fn detect_with_hints(
    call: &Call<'_, '_>,
    name: &str,
    chain: &str,
    hints: ProviderHints,
    bindings: &Bindings,
) -> Option<Detection> {
    detect_amqp(call, name, chain, hints, bindings)
        .map(|mut detection| {
            if hints.rabbitmq {
                detection.transport = Transport::RabbitMq;
            }
            detection
        })
        .or_else(|| detect_aws(call, name, chain, hints))
        .or_else(|| detect_kafka(call, name, chain, hints))
        .or_else(|| detect_jms(call, name, chain, hints, bindings))
        .or_else(|| detect_nats(call, name, chain, hints))
}

fn call_transport_candidates(
    call: &Call<'_, '_>,
    name: &str,
    chain: &str,
    hints: ProviderHints,
    bindings: &Bindings,
) -> BTreeSet<Transport> {
    let mut candidates = BTreeSet::new();
    if let Some(receiver) = call.receiver.as_ref()
        && let Some(transports) = bindings.transports.get(receiver)
    {
        candidates.extend(transports);
    }
    for identifier in chain.split(['.', ':']) {
        if let Some(transports) = bindings.transports.get(identifier) {
            candidates.extend(transports);
        }
    }
    if let Some(transports) = bindings.transports.get(&call.name) {
        candidates.extend(transports);
    }
    for transport in [
        Transport::Kafka,
        Transport::Amqp,
        Transport::RabbitMq,
        Transport::Jms,
        Transport::Nats,
        Transport::Sqs,
        Transport::Sns,
    ] {
        if provider_text_matches(transport, chain) {
            candidates.insert(transport);
        }
    }
    if !candidates.is_empty() {
        return candidates;
    }
    match name {
        "publish" => {
            candidates.extend(
                [
                    (Transport::Amqp, hints.amqp),
                    (Transport::RabbitMq, hints.rabbitmq),
                    (Transport::Nats, hints.nats),
                    (Transport::Sns, hints.sns),
                ]
                .into_iter()
                .filter_map(|(transport, present)| present.then_some(transport)),
            );
        }
        "subscribe" => {
            candidates.extend(
                [
                    (Transport::Kafka, hints.kafka),
                    (Transport::Nats, hints.nats),
                    (Transport::Sns, hints.sns),
                ]
                .into_iter()
                .filter_map(|(transport, present)| present.then_some(transport)),
            );
        }
        "createqueue" => {
            candidates.extend(
                [
                    (Transport::Amqp, hints.amqp),
                    (Transport::RabbitMq, hints.rabbitmq),
                    (Transport::Jms, hints.jms),
                    (Transport::Sqs, hints.sqs),
                ]
                .into_iter()
                .filter_map(|(transport, present)| present.then_some(transport)),
            );
        }
        "createtopic" => {
            candidates.extend(
                [(Transport::Jms, hints.jms), (Transport::Sns, hints.sns)]
                    .into_iter()
                    .filter_map(|(transport, present)| present.then_some(transport)),
            );
        }
        "send" => {
            candidates.extend(
                [(Transport::Kafka, hints.kafka), (Transport::Jms, hints.jms)]
                    .into_iter()
                    .filter_map(|(transport, present)| present.then_some(transport)),
            );
        }
        _ => {}
    }
    candidates
}

const fn hints_for(transport: Transport) -> ProviderHints {
    ProviderHints {
        kafka: matches!(transport, Transport::Kafka),
        amqp: matches!(transport, Transport::Amqp),
        rabbitmq: matches!(transport, Transport::RabbitMq),
        jms: matches!(transport, Transport::Jms),
        nats: matches!(transport, Transport::Nats),
        sqs: matches!(transport, Transport::Sqs),
        sns: matches!(transport, Transport::Sns),
    }
}

fn provider_text_matches(transport: Transport, text: &str) -> bool {
    match transport {
        Transport::Kafka => ["kafka", "kafkajs", "rdkafka", "sarama", "confluent_kafka"]
            .iter()
            .any(|needle| text.contains(needle)),
        Transport::Amqp => {
            text.contains("amqp")
                && !["amqplib", "rabbitmq", "rabbit", "pika", "lapin", "kombu"]
                    .iter()
                    .any(|needle| text.contains(needle))
        }
        Transport::RabbitMq => ["amqplib", "rabbitmq", "rabbit", "pika", "lapin", "kombu"]
            .iter()
            .any(|needle| text.contains(needle)),
        Transport::Jms => ["jms", "activemq", "artemis"]
            .iter()
            .any(|needle| text.contains(needle)),
        Transport::Nats => text.contains("nats"),
        Transport::Sqs => text.contains("sqs"),
        Transport::Sns => text.contains("sns"),
    }
}

fn is_import_keyword(identifier: &str) -> bool {
    [
        "as", "const", "from", "import", "let", "mut", "new", "package", "require", "use", "var",
    ]
    .iter()
    .any(|keyword| identifier.eq_ignore_ascii_case(keyword))
}

fn detect_kafka(
    call: &Call<'_, '_>,
    name: &str,
    chain: &str,
    hints: ProviderHints,
) -> Option<Detection> {
    let explicit = [
        "producerrecord",
        "futurerecord",
        "baserecord",
        "consumepartition",
        "subscribetopics",
        "addconsumetopics",
        "kafkaconsumer",
        "newwriter",
        "newreader",
    ]
    .iter()
    .any(|marker| chain.contains(marker));
    if !hints.kafka && !explicit {
        return None;
    }
    let group = property(call, &["groupid", "group_id", "group"]);
    let (role, entity, resources) = match name {
        "send" | "produce" if property(call, &["topic"]).is_some() || hints.kafka => (
            Role::Producer,
            Entity::Topic,
            resource_values(call, &["topic"], 0, bindings_none()),
        ),
        "to" if chain.contains("record") => (
            Role::Producer,
            Entity::Topic,
            positional_values(call, 0, bindings_none()),
        ),
        "producerrecord" => (
            Role::Producer,
            Entity::Topic,
            positional_values(call, 0, bindings_none()),
        ),
        "subscribe" | "subscribetopics" | "addconsumetopics" | "kafkalistener" => (
            Role::Consumer,
            Entity::Topic,
            resource_values(call, &["topic", "topics"], 0, bindings_none()),
        ),
        "kafkaconsumer" | "consumepartition" => (
            Role::Consumer,
            Entity::Topic,
            positional_values(call, 0, bindings_none()),
        ),
        "newwriter" => (
            Role::Producer,
            Entity::Topic,
            resource_values(call, &["topic"], 0, bindings_none()),
        ),
        "newreader" => (
            Role::Consumer,
            Entity::Topic,
            resource_values(call, &["topic"], 0, bindings_none()),
        ),
        "consumer" if property(call, &["groupid", "group_id"]).is_some() => {
            return Some(Detection {
                transport: Transport::Kafka,
                entity: Entity::Topic,
                role: Role::Declare,
                resources: Vec::new(),
                exchange: None,
                routing_key: None,
                consumer_group: group,
                certainty: Certainty::Exact,
                uncertainty: None,
            });
        }
        _ => return None,
    };
    Some(Detection {
        transport: Transport::Kafka,
        entity,
        role,
        resources: non_empty_resources(resources),
        exchange: None,
        routing_key: None,
        consumer_group: group,
        certainty: Certainty::Exact,
        uncertainty: None,
    })
}

#[allow(clippy::too_many_lines)]
fn detect_amqp(
    call: &Call<'_, '_>,
    name: &str,
    chain: &str,
    hints: ProviderHints,
    bindings: &Bindings,
) -> Option<Detection> {
    let explicit = [
        "assertqueue",
        "queuedeclare",
        "queue_declare",
        "assertexchange",
        "exchangedeclare",
        "exchange_declare",
        "bindqueue",
        "queuebind",
        "queue_bind",
        "sendtoqueue",
        "basicpublish",
        "basic_publish",
        "basicconsume",
        "basic_consume",
        "rabbitlistener",
    ]
    .contains(&name);
    if !hints.amqp && !hints.rabbitmq && !explicit && !chain.contains("amqp") {
        return None;
    }
    let positional = |index| positional_values(call, index, bindings);
    let detection = match name {
        "assertqueue" | "queuedeclare" | "queue_declare" | "createqueue" => Detection {
            transport: Transport::Amqp,
            entity: Entity::Queue,
            role: Role::Declare,
            resources: non_empty_resources(resource_values(call, &["queue", "name"], 0, bindings)),
            exchange: None,
            routing_key: None,
            consumer_group: None,
            certainty: Certainty::Exact,
            uncertainty: None,
        },
        "assertexchange" | "exchangedeclare" | "exchange_declare" => Detection {
            transport: Transport::Amqp,
            entity: Entity::Exchange,
            role: Role::Declare,
            resources: non_empty_resources(resource_values(
                call,
                &["exchange", "name"],
                0,
                bindings,
            )),
            exchange: None,
            routing_key: None,
            consumer_group: None,
            certainty: Certainty::Exact,
            uncertainty: None,
        },
        "bindqueue" | "queuebind" | "queue_bind" => {
            let queue = first_value(call, &["queue"], 0, bindings);
            let go_order = call.name == "QueueBind";
            let exchange = first_value(call, &["exchange"], if go_order { 2 } else { 1 }, bindings);
            let routing_key = first_value(
                call,
                &["routingkey", "routing_key", "bindingkey"],
                if go_order { 1 } else { 2 },
                bindings,
            );
            Detection {
                transport: Transport::Amqp,
                entity: Entity::Binding,
                role: Role::Bind,
                resources: vec![queue],
                exchange,
                routing_key,
                consumer_group: None,
                certainty: Certainty::Exact,
                uncertainty: None,
            }
        }
        "sendtoqueue" | "send_to_queue" => Detection {
            transport: Transport::Amqp,
            entity: Entity::Queue,
            role: Role::Producer,
            resources: non_empty_resources(positional(0)),
            exchange: None,
            routing_key: None,
            consumer_group: None,
            certainty: Certainty::Exact,
            uncertainty: None,
        },
        "consume" | "basicconsume" | "basic_consume" | "rabbitlistener" => Detection {
            transport: Transport::Amqp,
            entity: Entity::Queue,
            role: Role::Consumer,
            resources: non_empty_resources(resource_values(
                call,
                &["queue", "queues"],
                0,
                bindings,
            )),
            exchange: None,
            routing_key: None,
            consumer_group: property(call, &["consumertag", "consumer_tag"]),
            certainty: Certainty::Exact,
            uncertainty: None,
        },
        "publish" | "basicpublish" | "basic_publish" | "publishwithcontext" | "convertandsend" => {
            let exchange = first_value(call, &["exchange"], 0, bindings);
            let routing_key = first_value(call, &["routingkey", "routing_key"], 1, bindings);
            Detection {
                transport: Transport::Amqp,
                entity: Entity::Exchange,
                role: Role::Producer,
                resources: vec![exchange.clone()],
                exchange,
                routing_key,
                consumer_group: None,
                certainty: Certainty::Exact,
                uncertainty: None,
            }
        }
        _ => return None,
    };
    Some(detection)
}

fn detect_jms(
    call: &Call<'_, '_>,
    name: &str,
    chain: &str,
    hints: ProviderHints,
    bindings: &Bindings,
) -> Option<Detection> {
    let explicit = [
        "jmslistener",
        "createtopic",
        "createqueue",
        "createproducer",
        "createconsumer",
        "convertandsend",
    ]
    .contains(&name);
    if !hints.jms && !explicit && !chain.contains("jms") {
        return None;
    }
    let (role, entity, resources) = match name {
        "createtopic" => (
            Role::Declare,
            Entity::Topic,
            positional_values(call, 0, bindings),
        ),
        "createqueue" => (
            Role::Declare,
            Entity::Queue,
            positional_values(call, 0, bindings),
        ),
        "createproducer" | "convertandsend" => (
            Role::Producer,
            destination_entity(call, bindings),
            positional_values(call, 0, bindings),
        ),
        "createconsumer" | "jmslistener" => (
            Role::Consumer,
            destination_entity(call, bindings),
            resource_values(call, &["destination"], 0, bindings),
        ),
        _ => return None,
    };
    Some(Detection {
        transport: Transport::Jms,
        entity,
        role,
        resources: non_empty_resources(resources),
        exchange: None,
        routing_key: None,
        consumer_group: property(call, &["subscription", "subscriptionname", "clientid"]),
        certainty: Certainty::Exact,
        uncertainty: None,
    })
}

fn detect_nats(
    call: &Call<'_, '_>,
    name: &str,
    chain: &str,
    hints: ProviderHints,
) -> Option<Detection> {
    if !hints.nats && !chain.contains("nats") {
        return None;
    }
    let role = match name {
        "publish" | "request" => Role::Producer,
        "subscribe" | "queuesubscribe" | "queue_subscribe" => Role::Consumer,
        _ => return None,
    };
    Some(Detection {
        transport: Transport::Nats,
        entity: Entity::Subject,
        role,
        resources: non_empty_resources(resource_values(call, &["subject"], 0, bindings_none())),
        exchange: None,
        routing_key: None,
        consumer_group: property(call, &["queue", "queue_group"]),
        certainty: Certainty::Exact,
        uncertainty: None,
    })
}

fn detect_aws(
    call: &Call<'_, '_>,
    name: &str,
    chain: &str,
    hints: ProviderHints,
) -> Option<Detection> {
    let queue_service_context =
        hints.sqs || chain.contains("sqs") || property(call, &["queueurl", "queue_url"]).is_some();
    if queue_service_context {
        let role = match name {
            "sendmessage" | "send_message" | "sendmessagebatch" | "send_message_batch" => {
                Role::Producer
            }
            "receivemessage" | "receive_message" => Role::Consumer,
            "createqueue" | "create_queue" => Role::Declare,
            _ => return None,
        };
        return Some(Detection {
            transport: Transport::Sqs,
            entity: Entity::Queue,
            role,
            resources: non_empty_resources(resource_values(
                call,
                &["queueurl", "queue_url", "queuename", "queue_name"],
                0,
                bindings_none(),
            )),
            exchange: None,
            routing_key: None,
            consumer_group: None,
            certainty: Certainty::Exact,
            uncertainty: None,
        });
    }
    let topic_service_context =
        hints.sns || chain.contains("sns") || property(call, &["topicarn", "topic_arn"]).is_some();
    if !topic_service_context {
        return None;
    }
    let role = match name {
        "publish" | "publishbatch" | "publish_batch" => Role::Producer,
        "subscribe" => Role::Bind,
        "createtopic" | "create_topic" => Role::Declare,
        _ => return None,
    };
    Some(Detection {
        transport: Transport::Sns,
        entity: if role == Role::Bind {
            Entity::Binding
        } else {
            Entity::Topic
        },
        role,
        resources: non_empty_resources(resource_values(
            call,
            &["topicarn", "topic_arn", "name"],
            0,
            bindings_none(),
        )),
        exchange: property(call, &["protocol"]),
        routing_key: property(call, &["endpoint"]),
        consumer_group: None,
        certainty: Certainty::Exact,
        uncertainty: None,
    })
}

fn destination_entity(call: &Call<'_, '_>, bindings: &Bindings) -> Entity {
    let Some(identifier) = positional_identifier(call, 0) else {
        return Entity::Destination;
    };
    bindings
        .resources
        .get(&identifier)
        .map_or(Entity::Destination, |(entity, _)| *entity)
}

fn remember_binding(
    tokens: &[Token],
    source: &str,
    name_index: usize,
    detection: &Detection,
    bindings: &mut Bindings,
) {
    let Some(variable) = assigned_variable(tokens, source, name_index) else {
        return;
    };
    if detection.role == Role::Declare
        && let Some(Some(resource)) = detection.resources.first()
    {
        bindings
            .resources
            .insert(variable.clone(), (detection.entity, resource.clone()));
    }
    if detection.transport == Transport::Kafka
        && let Some(group) = detection.consumer_group.as_ref()
    {
        bindings.consumer_groups.insert(variable, group.clone());
    }
}

fn remember_transport_origin(
    tokens: &[Token],
    source: &str,
    name_index: usize,
    call: &Call<'_, '_>,
    detections: &[Detection],
    bindings: &mut Bindings,
) {
    let Some(variable) = assigned_variable(tokens, source, name_index) else {
        return;
    };
    let mut transports = detections
        .iter()
        .map(|detection| detection.transport)
        .collect::<BTreeSet<_>>();
    if let Some(receiver) = call.receiver.as_ref()
        && let Some(origins) = bindings.transports.get(receiver)
    {
        transports.extend(origins);
    }
    if let Some(origins) = bindings.transports.get(&call.name) {
        transports.extend(origins);
    }
    if transports.is_empty()
        && let Some(configured) = configured_transport(call)
    {
        transports.insert(configured);
    }
    if !transports.is_empty() {
        bindings
            .transports
            .entry(variable)
            .or_default()
            .extend(transports);
    }
}

fn configured_transport(call: &Call<'_, '_>) -> Option<Transport> {
    if !matches!(
        call.name.to_ascii_lowercase().as_str(),
        "client" | "connect" | "createclient" | "create_client" | "new"
    ) {
        return None;
    }
    let configured = call
        .args
        .iter()
        .filter(|token| token.kind == TokenKind::String)
        .filter_map(|token| literal_value(token.text(call.source)))
        .map(|value| value.to_ascii_lowercase())
        .collect::<Vec<_>>()
        .join(" ");
    [
        Transport::Kafka,
        Transport::Amqp,
        Transport::RabbitMq,
        Transport::Jms,
        Transport::Nats,
        Transport::Sqs,
        Transport::Sns,
    ]
    .into_iter()
    .find(|transport| provider_text_matches(*transport, &configured))
}

fn assigned_variable(tokens: &[Token], source: &str, name_index: usize) -> Option<String> {
    let start = name_index.saturating_sub(12);
    let equals = (start..name_index)
        .rev()
        .find(|index| tokens[*index].text(source) == "=")?;
    (start..equals).rev().find_map(|index| {
        (tokens[index].kind == TokenKind::Identifier).then(|| tokens[index].text(source).to_owned())
    })
}

fn call_name_index(tokens: &[Token], source: &str, open: usize) -> Option<usize> {
    let previous = open.checked_sub(1)?;
    if tokens[previous].kind == TokenKind::Identifier {
        return Some(previous);
    }
    if tokens[previous].text(source) == ">" {
        let mut depth = 0_usize;
        for index in (0..previous).rev() {
            match tokens[index].text(source) {
                ">" => depth += 1,
                "<" if depth == 0 => {
                    return index
                        .checked_sub(1)
                        .filter(|candidate| tokens[*candidate].kind == TokenKind::Identifier);
                }
                "<" => depth = depth.saturating_sub(1),
                _ => {}
            }
        }
    }
    (tokens[previous].text(source) == "!")
        .then(|| previous.checked_sub(1))
        .flatten()
        .filter(|index| tokens[*index].kind == TokenKind::Identifier)
}

fn matching_close(tokens: &[Token], source: &str, open: usize) -> Option<usize> {
    let mut depth = 0_usize;
    for (index, token) in tokens.iter().enumerate().skip(open) {
        match token.text(source) {
            "(" => depth += 1,
            ")" => {
                depth = depth.saturating_sub(1);
                if depth == 0 {
                    return Some(index);
                }
            }
            _ => {}
        }
    }
    None
}

fn call_chain(tokens: &[Token], source: &str, name_index: usize) -> String {
    let mut start = name_index;
    loop {
        if start >= 2
            && tokens[start - 1].text(source) == "."
            && tokens[start - 2].kind == TokenKind::Identifier
        {
            start -= 2;
        } else if start >= 3
            && tokens[start - 1].text(source) == ":"
            && tokens[start - 2].text(source) == ":"
            && tokens[start - 3].kind == TokenKind::Identifier
        {
            start -= 3;
        } else {
            break;
        }
    }
    tokens[start..=name_index]
        .iter()
        .map(|token| token.text(source))
        .collect::<String>()
}

fn receiver_name(tokens: &[Token], source: &str, name_index: usize) -> Option<String> {
    if name_index >= 2
        && tokens[name_index - 1].text(source) == "."
        && tokens[name_index - 2].kind == TokenKind::Identifier
    {
        return Some(tokens[name_index - 2].text(source).to_owned());
    }
    (name_index >= 3
        && tokens[name_index - 1].text(source) == ":"
        && tokens[name_index - 2].text(source) == ":"
        && tokens[name_index - 3].kind == TokenKind::Identifier)
        .then(|| tokens[name_index - 3].text(source).to_owned())
}

fn resource_values(
    call: &Call<'_, '_>,
    properties: &[&str],
    positional: usize,
    bindings: &Bindings,
) -> Vec<Option<String>> {
    let properties = property_values(call, properties);
    if properties.is_empty() {
        positional_values(call, positional, bindings)
    } else {
        properties.into_iter().map(Some).collect()
    }
}

fn first_value(
    call: &Call<'_, '_>,
    properties: &[&str],
    positional: usize,
    bindings: &Bindings,
) -> Option<String> {
    resource_values(call, properties, positional, bindings)
        .into_iter()
        .flatten()
        .next()
}

fn property(call: &Call<'_, '_>, names: &[&str]) -> Option<String> {
    property_values(call, names).into_iter().next()
}

fn property_values(call: &Call<'_, '_>, names: &[&str]) -> Vec<String> {
    let names = names
        .iter()
        .map(|name| name.to_ascii_lowercase())
        .collect::<BTreeSet<_>>();
    let mut values = Vec::new();
    for index in 0..call.args.len() {
        if call.args[index].kind != TokenKind::Identifier
            || !names.contains(&call.args[index].text(call.source).to_ascii_lowercase())
        {
            continue;
        }
        let Some(separator) = call.args.get(index + 1) else {
            continue;
        };
        if !matches!(separator.text(call.source), ":" | "=" | "(") {
            continue;
        }
        let mut depth = 0_i32;
        for token in call.args.iter().skip(index + 2) {
            match token.text(call.source) {
                "[" | "{" | "(" => depth += 1,
                "]" | "}" | ")" => depth -= 1,
                "," if depth <= 0 => break,
                _ => {}
            }
            if token.kind == TokenKind::String
                && let Some(value) = literal_value(token.text(call.source))
            {
                values.push(value);
            }
        }
    }
    values
}

fn positional_values(
    call: &Call<'_, '_>,
    position: usize,
    bindings: &Bindings,
) -> Vec<Option<String>> {
    let segments = argument_segments(call.args, call.source);
    let Some(segment) = segments.get(position) else {
        return vec![None];
    };
    let literals = segment
        .iter()
        .filter(|token| token.kind == TokenKind::String)
        .filter_map(|token| literal_value(token.text(call.source)))
        .map(Some)
        .collect::<Vec<_>>();
    if !literals.is_empty() {
        return literals;
    }
    let identifier = segment.iter().find_map(|token| {
        (token.kind == TokenKind::Identifier).then(|| token.text(call.source).to_owned())
    });
    if let Some(identifier) = identifier
        && let Some((_, value)) = bindings.resources.get(&identifier)
    {
        return vec![Some(value.clone())];
    }
    vec![None]
}

fn positional_identifier(call: &Call<'_, '_>, position: usize) -> Option<String> {
    argument_segments(call.args, call.source)
        .get(position)?
        .iter()
        .find_map(|token| {
            (token.kind == TokenKind::Identifier).then(|| token.text(call.source).to_owned())
        })
}

fn argument_segments<'tokens>(tokens: &'tokens [Token], source: &str) -> Vec<&'tokens [Token]> {
    let mut segments = Vec::new();
    let mut start = 0_usize;
    let mut depth = 0_i32;
    for (index, token) in tokens.iter().enumerate() {
        match token.text(source) {
            "(" | "[" | "{" => depth += 1,
            ")" | "]" | "}" => depth -= 1,
            "," if depth == 0 => {
                segments.push(&tokens[start..index]);
                start = index + 1;
            }
            _ => {}
        }
    }
    if start < tokens.len() {
        segments.push(&tokens[start..]);
    }
    segments
}

fn literal_value(raw: &str) -> Option<String> {
    let trimmed = raw.trim();
    let quote = trimmed.find(['"', '\'', '`'])?;
    let mark = trimmed.as_bytes().get(quote).copied()? as char;
    let tail = &trimmed[quote + mark.len_utf8()..];
    let end = tail.rfind(mark)?;
    let value = &tail[..end];
    if value.is_empty() || value.contains("${") || value.contains("#{") {
        None
    } else {
        Some(value.to_owned())
    }
}

fn source_line(source: &str, line: u32) -> String {
    source
        .lines()
        .nth(usize::try_from(line.saturating_sub(1)).unwrap_or(usize::MAX))
        .map(str::trim)
        .unwrap_or_default()
        .chars()
        .take(300)
        .collect()
}

fn non_empty_resources(resources: Vec<Option<String>>) -> Vec<Option<String>> {
    if resources.is_empty() {
        vec![None]
    } else {
        resources
    }
}

fn bindings_none() -> &'static Bindings {
    static EMPTY: std::sync::OnceLock<Bindings> = std::sync::OnceLock::new();
    EMPTY.get_or_init(Bindings::default)
}

fn add_graph_fallbacks(
    repository: &str,
    state: &RepositoryState,
    args: &Value,
    source_keys: &BTreeSet<(String, String, Role, String, u32)>,
    max_observations: usize,
    observations: &mut Vec<Observation>,
) {
    for (slot, node) in state.graph().nodes().iter().enumerate() {
        let entity = match node.kind {
            NodeKind::Topic => Entity::Topic,
            NodeKind::Queue => Entity::Queue,
            NodeKind::Exchange => Entity::Exchange,
            NodeKind::Binding => Entity::Binding,
            _ => continue,
        };
        if !super::node_is_visible(state, slot, args) {
            continue;
        }
        let index = weavatrix_graph::NodeIndex::new(u32::try_from(slot).unwrap_or(u32::MAX));
        for edge in state.graph().incoming_at(index) {
            let role = match edge.kind {
                EdgeKind::Publishes => Role::Producer,
                EdgeKind::Consumes => Role::Consumer,
                EdgeKind::Binds => Role::Bind,
                EdgeKind::Configures => Role::Declare,
                _ => continue,
            };
            let span = edge.provenance.span.as_ref();
            let path = span.map(|span| span.file.clone()).unwrap_or_default();
            let Some(language) = language_for_path(&path) else {
                continue;
            };
            let line = span.map_or(0, |span| span.start.line);
            if source_keys.contains(&(
                repository.to_owned(),
                path.clone(),
                role,
                node.label.clone(),
                line,
            )) {
                continue;
            }
            let candidates = fallback_transport_candidates(entity);
            for transport in &candidates {
                let ambiguous = candidates.len() > 1;
                observations.push(Observation {
                    repository: repository.to_owned(),
                    path: path.clone(),
                    line,
                    column: span.map_or(0, |span| span.start.column),
                    language: language.as_str().to_owned(),
                    transport: *transport,
                    entity,
                    role,
                    resource: Some(node.label.clone()),
                    exchange: None,
                    routing_key: None,
                    consumer_group: None,
                    receiver: None,
                    evidence: edge.provenance.detail.clone().unwrap_or_default(),
                    origin: "graph_domain",
                    certainty: if ambiguous {
                        Certainty::Ambiguous
                    } else {
                        Certainty::Derived
                    },
                    uncertainty: ambiguous.then(|| {
                        "graph-domain evidence identifies the destination kind but has multiple concrete transport candidates"
                            .to_owned()
                    }),
                    candidates: candidates.clone(),
                    runtime_observed: false,
                });
                if observations.len() >= max_observations {
                    return;
                }
            }
        }
    }
}

fn fallback_transport_candidates(entity: Entity) -> BTreeSet<Transport> {
    match entity {
        Entity::Topic => BTreeSet::from([Transport::Kafka, Transport::Jms, Transport::Sns]),
        Entity::Queue => BTreeSet::from([
            Transport::Amqp,
            Transport::RabbitMq,
            Transport::Jms,
            Transport::Sqs,
        ]),
        Entity::Exchange | Entity::Binding => {
            BTreeSet::from([Transport::Amqp, Transport::RabbitMq])
        }
        Entity::Subject => BTreeSet::from([Transport::Nats]),
        Entity::Destination => BTreeSet::from([Transport::Jms]),
    }
}

fn observation_identity(observation: &Observation) -> Option<(String, String, Role, String, u32)> {
    observation.resource.as_ref().map(|resource| {
        (
            observation.repository.clone(),
            observation.path.clone(),
            observation.role,
            resource.clone(),
            observation.line,
        )
    })
}

fn bounded_usize(args: &Value, key: &str, default: usize, maximum: usize) -> usize {
    args.get(key)
        .and_then(Value::as_u64)
        .and_then(|value| usize::try_from(value).ok())
        .unwrap_or(default)
        .clamp(1, maximum)
}

fn bounded_u64(args: &Value, key: &str, default: u64, maximum: u64) -> u64 {
    args.get(key)
        .and_then(Value::as_u64)
        .unwrap_or(default)
        .clamp(1, maximum)
}

#[cfg(test)]
#[path = "../../tests/support/transport_fixture.rs"]
mod transport_fixture;

#[cfg(test)]
mod tests {
    use super::{
        Entity, Role, Transport, event_contracts, extract_observations, load_runtime_evidence,
        may_contain_transport_observation, parse_rfc3339_millis, routing_keys_match,
    };
    use crate::Weavatrix;
    use blazingly_json::{Value, json};
    use std::time::{SystemTime, UNIX_EPOCH};
    use weavatrix_parse::Language;

    use super::transport_fixture::TempRepository;

    #[test]
    fn prefilter_keeps_every_detectable_transport_family() {
        for source in [
            "import { Kafka } from 'kafkajs'; producer.send({topic: 'x'});",
            "channel.assertQueue('x');",
            "@JmsListener(destination = \"x\")",
            "nats.publish('x', payload)",
            "client.sendMessage({QueueUrl: queue})",
            "client.publish({TopicArn: topic})",
            "session.createQueue('x')",
        ] {
            assert!(
                may_contain_transport_observation(source),
                "transport source was filtered: {source}"
            );
        }
        assert!(!may_contain_transport_observation(
            "fn calculate_total() { println!(\"done\"); }"
        ));
    }

    #[test]
    fn extracts_kafka_roles_groups_and_ignores_comments() {
        let source = r#"
            import { Kafka } from "kafkajs";
            const consumer = kafka.consumer({ groupId: "billing-v2" });
            await producer.send({ topic: "invoice.created", messages: [] });
            await consumer.subscribe({ topics: ["invoice.created", "invoice.retry"] });
            // producer.send({ topic: "not-real" });
        "#;
        let observations =
            extract_observations("repo", "src/events.ts", Language::TypeScript, source);
        assert!(observations.iter().any(|item| {
            item.transport == Transport::Kafka
                && item.role == Role::Producer
                && item.resource.as_deref() == Some("invoice.created")
        }));
        assert!(observations.iter().any(|item| {
            item.role == Role::Consumer
                && item.resource.as_deref() == Some("invoice.retry")
                && item.consumer_group.as_deref() == Some("billing-v2")
        }));
        assert!(
            observations
                .iter()
                .all(|item| item.resource.as_deref() != Some("not-real"))
        );
    }

    #[test]
    fn extracts_rabbitmq_binding_and_routing_semantics() {
        let source = r#"
            import amqp from "amqplib";
            channel.assertExchange("events", "topic");
            channel.assertQueue("mail");
            channel.bindQueue("mail", "events", "user.*");
            channel.publish("events", "user.created", body);
            channel.consume("mail", handler);
        "#;
        let observations = extract_observations("repo", "events.js", Language::JavaScript, source);
        let binding = observations
            .iter()
            .find(|item| item.role == Role::Bind)
            .expect("binding");
        assert_eq!(binding.transport, Transport::RabbitMq);
        assert_eq!(binding.entity, Entity::Binding);
        assert_eq!(binding.resource.as_deref(), Some("mail"));
        assert_eq!(binding.exchange.as_deref(), Some("events"));
        assert_eq!(binding.routing_key.as_deref(), Some("user.*"));
        assert!(routing_keys_match(Some("user.created"), Some("user.*")));
        assert!(!routing_keys_match(Some("invoice.created"), Some("user.*")));
    }

    #[test]
    fn keeps_generic_amqp_distinct_from_rabbitmq() {
        let source = r#"
            import amqpClient from "amqp-client";
            const connection = amqpClient.connect("amqp://broker");
            connection.publish("events", "user.created", body);
        "#;
        let observations = extract_observations("repo", "events.ts", Language::TypeScript, source);
        let published = observations
            .iter()
            .find(|item| item.role == Role::Producer)
            .expect("generic AMQP producer");
        assert_eq!(published.transport, Transport::Amqp);
        assert_eq!(published.resource.as_deref(), Some("events"));
        assert_eq!(
            published.candidates,
            std::collections::BTreeSet::from([Transport::Amqp])
        );
    }

    #[test]
    fn classifies_sqs_and_sns_as_distinct_aws_transports() {
        let source = r#"
            import { SQSClient } from "@aws-sdk/client-sqs";
            import { SNSClient } from "@aws-sdk/client-sns";
            const queue = new SQSClient({});
            const topic = new SNSClient({});
            queue.sendMessage({ QueueUrl: "https://sqs.eu-west-1.amazonaws.com/1/orders" });
            topic.publish({ TopicArn: "arn:aws:sns:eu-west-1:1:orders" });
        "#;
        let observations =
            extract_observations("repo", "aws-events.ts", Language::TypeScript, source);
        assert!(observations.iter().any(|item| {
            item.transport == Transport::Sqs
                && item.role == Role::Producer
                && item.resource.as_deref() == Some("https://sqs.eu-west-1.amazonaws.com/1/orders")
        }));
        assert!(observations.iter().any(|item| {
            item.transport == Transport::Sns
                && item.role == Role::Producer
                && item.resource.as_deref() == Some("arn:aws:sns:eu-west-1:1:orders")
        }));
    }

    #[test]
    fn extracts_python_go_java_and_rust_api_shapes() {
        let cases = [
            (
                Language::Python,
                "producer.py",
                "from kafka import KafkaProducer\nproducer.send('orders')",
                Transport::Kafka,
                Role::Producer,
                "orders",
            ),
            (
                Language::Go,
                "consumer.go",
                "import \"github.com/IBM/sarama\"\nconsumer.ConsumePartition(\"orders\", 0, 0)",
                Transport::Kafka,
                Role::Consumer,
                "orders",
            ),
            (
                Language::Java,
                "Listener.java",
                "@JmsListener(destination = \"mail\")\nvoid receive() {}",
                Transport::Jms,
                Role::Consumer,
                "mail",
            ),
            (
                Language::Rust,
                "producer.rs",
                "use rdkafka::producer::FutureRecord;\nFutureRecord::to(\"orders\");",
                Transport::Kafka,
                Role::Producer,
                "orders",
            ),
        ];
        for (language, path, source, transport, role, resource) in cases {
            let observations = extract_observations("repo", path, language, source);
            assert!(
                observations.iter().any(|item| {
                    item.transport == transport
                        && item.role == role
                        && item.resource.as_deref() == Some(resource)
                }),
                "missing {path}: {observations:#?}"
            );
        }
    }

    #[test]
    fn keeps_computed_destinations_as_explicit_ambiguities() {
        let source = "import nats\nawait nats.publish(subject_name, payload)\nawait nats.subscribe(subject())";
        let observations = extract_observations("repo", "worker.py", Language::Python, source);
        assert_eq!(observations.len(), 2);
        assert!(observations.iter().all(|item| {
            item.resource.is_none()
                && item.certainty == super::Certainty::Ambiguous
                && item.candidates == std::collections::BTreeSet::from([Transport::Nats])
                && item
                    .uncertainty
                    .as_deref()
                    .is_some_and(|reason| reason.contains("requires runtime evidence"))
        }));
    }

    #[test]
    fn resolves_same_publish_name_from_alias_and_receiver_data_flow() {
        let source = r#"
            import * as natsLib from "nats";
            import rabbitAlias from "amqplib";
            import { SNSClient as Notifications } from "@aws-sdk/client-sns";

            const nc = natsLib.connect();
            const connection = rabbitAlias.connect("amqp://localhost");
            const channel = connection.createChannel();
            const sns = new Notifications({});

            nc.publish("orders.created", payload);
            channel.publish("events", "orders.created", payload);
            sns.publish({ TopicArn: "arn:aws:sns:eu-west-1:1:orders" });
        "#;
        let observations = extract_observations("repo", "mixed.ts", Language::TypeScript, source);
        let find = |transport, resource| {
            observations.iter().any(|item| {
                item.transport == transport
                    && item.role == Role::Producer
                    && item.resource.as_deref() == Some(resource)
            })
        };
        assert!(find(Transport::Nats, "orders.created"));
        assert!(find(Transport::RabbitMq, "events"));
        assert!(find(Transport::Sns, "arn:aws:sns:eu-west-1:1:orders"));
        assert!(observations.iter().all(|item| {
            matches!(
                item.transport,
                Transport::Kafka
                    | Transport::Amqp
                    | Transport::RabbitMq
                    | Transport::Jms
                    | Transport::Nats
                    | Transport::Sqs
                    | Transport::Sns
            )
        }));
    }

    #[test]
    fn preserves_a_typed_union_for_a_polymorphic_receiver() {
        let source = r#"
            import * as natsLib from "nats";
            import rabbitAlias from "amqplib";
            const nc = natsLib.connect();
            const connection = rabbitAlias.connect();
            const channel = connection.createChannel();
            const bus = useNats ? nc : channel;
            bus.publish("events", "route");
        "#;
        let observations = extract_observations("repo", "union.ts", Language::TypeScript, source);
        let union = observations
            .iter()
            .filter(|item| item.line == 8)
            .collect::<Vec<_>>();
        assert_eq!(union.len(), 2, "{observations:#?}");
        assert!(union.iter().all(|item| {
            item.certainty == super::Certainty::Ambiguous
                && item
                    .uncertainty
                    .as_deref()
                    .is_some_and(|reason| reason.contains("multiple concrete transports"))
        }));
        assert!(union.iter().all(|item| {
            item.candidates
                == std::collections::BTreeSet::from([Transport::RabbitMq, Transport::Nats])
        }));
        assert_eq!(
            union
                .iter()
                .map(|item| item.transport)
                .collect::<std::collections::BTreeSet<_>>(),
            std::collections::BTreeSet::from([Transport::RabbitMq, Transport::Nats])
        );
    }

    #[test]
    fn validates_and_normalizes_revision_bound_runtime_evidence() {
        let repository = TempRepository::new();
        let engine = Weavatrix::open(repository.root()).expect("analyze");
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock")
            .as_millis();
        let report = json!({
            "schema": "weavatrix.transport-runtime.v1",
            "repositoryRevision": engine.state().snapshot().revision,
            "generatedAt": u64::try_from(now).expect("timestamp"),
            "coverage": {"event": "COMPLETE"},
            "observations": [{
                "transport": "event",
                "system": "nats",
                "side": "publisher",
                "name": "orders.created",
                "file": "src/events.ts",
                "line": 2,
                "observedCount": 4
            }],
            "otlp": {
                "resourceSpans": [{
                    "scopeSpans": [{
                        "spans": [{
                            "kind": 5,
                            "attributes": [
                                {"key": "messaging.system", "value": {"stringValue": "kafka"}},
                                {"key": "messaging.destination.name", "value": {"stringValue": "orders.retry"}},
                                {"key": "messaging.operation.type", "value": {"stringValue": "receive"}},
                                {"key": "messaging.kafka.consumer.group", "value": {"stringValue": "billing"}},
                                {"key": "code.file.path", "value": {"stringValue": "src/events.ts"}},
                                {"key": "code.line.number", "value": {"intValue": "2"}}
                            ]
                        }]
                    }]
                }]
            }
        });
        repository.write_runtime_report(&blazingly_json::to_vec(&report).expect("serialize"));
        let loaded = load_runtime_evidence("backend", engine.state(), &json!({}));
        assert_eq!(loaded.status, "COMPLETE", "{:?}", loaded.reasons);
        assert_eq!(loaded.observations.len(), 2);
        let observation = loaded
            .observations
            .iter()
            .find(|item| item.transport == Transport::Nats)
            .expect("NATS observation");
        assert_eq!(observation.transport, Transport::Nats);
        assert_eq!(observation.role, Role::Producer);
        assert_eq!(observation.resource.as_deref(), Some("orders.created"));
        assert_eq!(observation.path, "src/events.ts");
        assert!(observation.runtime_observed);
        let otlp = loaded
            .observations
            .iter()
            .find(|item| item.transport == Transport::Kafka)
            .expect("OTLP Kafka observation");
        assert_eq!(otlp.role, Role::Consumer);
        assert_eq!(otlp.resource.as_deref(), Some("orders.retry"));
        assert_eq!(otlp.consumer_group.as_deref(), Some("billing"));
    }

    #[test]
    fn rejects_stale_revision_and_path_escape_runtime_reports() {
        let repository = TempRepository::new();
        let engine = Weavatrix::open(repository.root()).expect("analyze");
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock")
            .as_millis();
        let report = json!({
            "schema": "weavatrix.transport-runtime.v1",
            "repositoryRevision": "wrong-revision",
            "generatedAt": u64::try_from(now).expect("timestamp"),
            "coverage": {"event": "COMPLETE"},
            "observations": [{
                "transport": "nats",
                "side": "publisher",
                "name": "orders.created"
            }]
        });
        repository.write_runtime_report(&blazingly_json::to_vec(&report).expect("serialize"));
        let loaded = load_runtime_evidence("backend", engine.state(), &json!({}));
        assert_eq!(loaded.status, "REJECTED");
        assert!(loaded.observations.is_empty());
        assert!(
            loaded
                .reasons
                .iter()
                .any(|reason| reason.contains("revision"))
        );
        let escaped = load_runtime_evidence(
            "backend",
            engine.state(),
            &json!({"runtime_evidence_files": {"backend": "../outside.json"}}),
        );
        assert_eq!(escaped.status, "REJECTED");
        assert!(escaped.reasons[0].contains("escapes"));
    }

    #[test]
    fn missing_runtime_evidence_is_explicit_and_not_a_static_coverage_failure() {
        let repository = TempRepository::new();
        let engine = Weavatrix::open(repository.root()).expect("analyze");
        let loaded = load_runtime_evidence("backend", engine.state(), &json!({}));
        assert_eq!(loaded.status, "NOT_PROVIDED");
        assert!(!loaded.present);
        assert!(loaded.observations.is_empty());
        assert!(
            loaded
                .reasons
                .iter()
                .any(|reason| reason.contains("no revision-bound runtime transport evidence"))
        );
    }

    #[test]
    fn public_event_contract_shape_has_explicit_evidence_without_forbidden_fallback_states() {
        let repository = TempRepository::new();
        let engine = Weavatrix::open(repository.root()).expect("analyze");
        let result = event_contracts(engine.state(), &[], &json!({})).unwrap();
        assert_eq!(result["status"], "COMPLETE");
        assert_eq!(result["runtimeEvidence"]["present"], false);
        assert!(
            result["runtimeEvidence"]["absenceReasons"]
                .as_array()
                .is_some_and(|reasons| !reasons.is_empty())
        );
        let ambiguity = result["ambiguous_evidence"]
            .as_array()
            .and_then(|items| items.first())
            .expect("computed NATS destination ambiguity");
        assert_eq!(ambiguity["classification"]["candidates"][0], "nats");
        assert!(
            ambiguity["evidence"]
                .as_str()
                .is_some_and(|line| !line.is_empty())
        );
        assert_no_forbidden_fallback_state(&result);
    }

    #[test]
    fn exact_tokenized_transport_suppresses_generic_graph_fallbacks() {
        let repository = TempRepository::new();
        std::fs::write(
            repository.root().join("src/events.ts"),
            concat!(
                "import nats from 'nats';\n",
                "nats.publish('jobs', body);\n",
                "nats.subscribe('jobs');\n",
            ),
        )
        .expect("source");
        let engine = Weavatrix::open(repository.root()).expect("analyze");
        let result = event_contracts(engine.state(), &[], &json!({})).unwrap();
        assert_eq!(result["totals"]["matches"], 1, "{result}");
        assert_eq!(result["totals"]["mismatches"], 0, "{result}");
        assert_eq!(result["totals"]["ambiguities"], 0, "{result}");
        assert_eq!(result["release_gate"], "PASS", "{result}");
    }

    #[test]
    fn graph_marker_prefilter_skips_non_transport_sources_without_losing_counts() {
        let repository = TempRepository::new();
        std::fs::write(
            repository.root().join("src/math.ts"),
            "export const sum = (left, right) => left + right;\n",
        )
        .expect("ordinary source");
        let engine = Weavatrix::open(repository.root()).expect("analyze");
        let result = event_contracts(engine.state(), &[], &json!({})).unwrap();

        assert_eq!(result["totals"]["files_considered"], 2);
        assert_eq!(result["totals"]["files_scanned"], 1);
        assert_eq!(result["totals"]["files_without_transport_markers"], 1);
    }

    #[test]
    fn same_revision_is_tokenized_once_for_backend_and_client_roles() {
        let repository = TempRepository::new();
        std::fs::write(
            repository.root().join("src/events.ts"),
            concat!(
                "import nats from 'nats';\n",
                "nats.publish('jobs', body);\n",
                "nats.subscribe('jobs');\n",
            ),
        )
        .expect("source");
        let engine = Weavatrix::open(repository.root()).expect("analyze");
        let backend_only = event_contracts(engine.state(), &[], &json!({})).unwrap();
        let client_name = "same-repository".to_owned();
        let with_client =
            event_contracts(engine.state(), &[(client_name, engine.state())], &json!({})).unwrap();

        assert_eq!(
            with_client["totals"]["files_considered"],
            backend_only["totals"]["files_considered"]
        );
        assert_eq!(
            with_client["totals"]["files_scanned"],
            backend_only["totals"]["files_scanned"]
        );
        assert!(
            with_client["totals"]["observations"].as_u64()
                > backend_only["totals"]["observations"].as_u64()
        );
    }

    fn assert_no_forbidden_fallback_state(value: &Value) {
        match value {
            Value::String(value) => {
                let forbidden = [
                    ["UN", "KNOWN"].concat(),
                    ["UN", "SUPPORTED"].concat(),
                    ["PAR", "TIAL"].concat(),
                    ["NOT", "_AVAILABLE"].concat(),
                ];
                assert!(
                    !forbidden.iter().any(|candidate| candidate == value),
                    "forbidden fallback state: {value}"
                );
            }
            Value::Array(values) => {
                for value in values {
                    assert_no_forbidden_fallback_state(value);
                }
            }
            Value::Object(values) => {
                for value in values.values() {
                    assert_no_forbidden_fallback_state(value);
                }
            }
            _ => {}
        }
    }

    #[test]
    fn parses_rfc3339_with_fraction_and_offset() {
        assert_eq!(parse_rfc3339_millis("1970-01-01T00:00:00.123Z"), Some(123));
        assert_eq!(parse_rfc3339_millis("1970-01-01T02:30:00+02:30"), Some(0));
        assert!(parse_rfc3339_millis("2026-02-31T00:00:00Z").is_none());
    }
}
