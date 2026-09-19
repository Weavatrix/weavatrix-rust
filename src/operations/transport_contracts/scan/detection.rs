use super::{
    BTreeSet, Bindings, Call, Certainty, Detection, ProviderHints, Token, TokenKind, Transport,
    detect_amqp, detect_aws, detect_jms, detect_kafka, detect_nats,
};

pub(super) fn propagate_assignment_binding(
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

pub(super) fn detect(
    call: &Call<'_, '_>,
    hints: ProviderHints,
    bindings: &Bindings,
) -> Vec<Detection> {
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

pub(super) fn detect_with_hints(
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

pub(super) fn call_transport_candidates(
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

pub(super) const fn hints_for(transport: Transport) -> ProviderHints {
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

pub(super) fn provider_text_matches(transport: Transport, text: &str) -> bool {
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

pub(super) fn is_import_keyword(identifier: &str) -> bool {
    [
        "as", "const", "from", "import", "let", "mut", "new", "package", "require", "use", "var",
    ]
    .iter()
    .any(|keyword| identifier.eq_ignore_ascii_case(keyword))
}
