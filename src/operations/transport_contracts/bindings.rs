use super::{
    BTreeMap, BTreeSet, Bindings, Call, Detection, ProviderHints, Role, Token, TokenKind,
    Transport, assigned_variable, is_import_keyword, literal_value, provider_text_matches,
};

impl ProviderHints {
    pub(super) fn from_bindings(bindings: &Bindings, tokens: &[Token], source: &str) -> Self {
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

impl Bindings {
    pub(super) fn from_tokens(tokens: &[Token], source: &str) -> Self {
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

pub(super) fn remember_binding(
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

pub(super) fn remember_transport_origin(
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

pub(super) fn configured_transport(call: &Call<'_, '_>) -> Option<Transport> {
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

pub(super) fn bindings_none() -> &'static Bindings {
    static EMPTY: std::sync::OnceLock<Bindings> = std::sync::OnceLock::new();
    EMPTY.get_or_init(Bindings::default)
}
