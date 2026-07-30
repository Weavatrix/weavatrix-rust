use super::{
    BTreeMap, BTreeSet, Certainty, Component, Entity, Observation, Path, RepositoryState, Role,
    Transport, Value, json, provider_text_matches,
};

pub(super) fn normalize_runtime_observation(
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

pub(super) fn runtime_transport(system: &str) -> Option<Transport> {
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

pub(super) fn runtime_entity(transport: Transport, kind: &str, role: Role) -> Entity {
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

pub(super) fn bounded_text(value: &Value, max: usize) -> Option<String> {
    let text = value.as_str()?.trim();
    (!text.is_empty()).then(|| text.chars().take(max).collect())
}

pub(super) fn safe_repository_path(root: &Path, candidate: &str) -> Option<std::path::PathBuf> {
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

pub(super) fn safe_source_path(root: &Path, candidate: &str) -> Option<String> {
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

pub(super) fn otlp_event_observations(report: &Value) -> impl Iterator<Item = Value> + '_ {
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

pub(super) fn otlp_event_observation(span: &Value) -> Option<Value> {
    let attributes = otlp_attributes(&span["attributes"]);
    let system = attributes.get("messaging.system")?.to_ascii_lowercase();
    let name = attributes
        .get("messaging.destination.name")
        .or_else(|| attributes.get("messaging.destination"))?
        .clone();
    let side = otlp_span_side(span)?;
    Some(json_object_observation(&attributes, &system, &name, side))
}

pub(super) fn json_object_observation(
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

pub(super) fn otlp_attributes(value: &Value) -> BTreeMap<String, String> {
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

pub(super) fn otlp_span_side(span: &Value) -> Option<&'static str> {
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
