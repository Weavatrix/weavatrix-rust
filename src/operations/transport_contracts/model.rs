use super::{BTreeMap, BTreeSet, Token, Value, json};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(super) enum Transport {
    Kafka,
    Amqp,
    RabbitMq,
    Jms,
    Nats,
    Sqs,
    Sns,
}

impl Transport {
    pub(super) const fn as_str(self) -> &'static str {
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

    pub(super) const fn contract_family(self) -> Self {
        match self {
            Self::RabbitMq => Self::Amqp,
            concrete => concrete,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(super) enum Entity {
    Topic,
    Queue,
    Exchange,
    Subject,
    Destination,
    Binding,
}

impl Entity {
    pub(super) const fn as_str(self) -> &'static str {
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
pub(super) enum Role {
    Producer,
    Consumer,
    Declare,
    Bind,
}

impl Role {
    pub(super) const fn as_str(self) -> &'static str {
        match self {
            Self::Producer => "producer",
            Self::Consumer => "consumer",
            Self::Declare => "declare",
            Self::Bind => "bind",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(super) enum Certainty {
    Exact,
    Derived,
    Ambiguous,
}

impl Certainty {
    pub(super) const fn as_str(self) -> &'static str {
        match self {
            Self::Exact => "exact",
            Self::Derived => "derived",
            Self::Ambiguous => "ambiguous",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub(super) struct Observation {
    pub(super) repository: String,
    pub(super) path: String,
    pub(super) line: u32,
    pub(super) column: u32,
    pub(super) language: String,
    pub(super) transport: Transport,
    pub(super) entity: Entity,
    pub(super) role: Role,
    pub(super) resource: Option<String>,
    pub(super) exchange: Option<String>,
    pub(super) routing_key: Option<String>,
    pub(super) consumer_group: Option<String>,
    pub(super) receiver: Option<String>,
    pub(super) evidence: String,
    pub(super) origin: &'static str,
    pub(super) certainty: Certainty,
    pub(super) uncertainty: Option<String>,
    pub(super) candidates: BTreeSet<Transport>,
    pub(super) runtime_observed: bool,
}

impl Observation {
    pub(super) fn key(&self) -> Option<ContractKey> {
        Some(ContractKey {
            transport: self.transport.contract_family(),
            entity: self.entity,
            resource: self.resource.clone()?,
        })
    }

    pub(super) fn to_json(&self) -> Value {
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
pub(super) struct ContractKey {
    pub(super) transport: Transport,
    pub(super) entity: Entity,
    pub(super) resource: String,
}

#[derive(Debug, Default)]
pub(super) struct ScanSummary {
    pub(super) files_considered: usize,
    pub(super) files_scanned: usize,
    pub(super) files_without_transport_markers: usize,
    pub(super) files_skipped_oversize: usize,
    pub(super) files_unreadable: usize,
    pub(super) files_without_transport_extractor: usize,
    pub(super) observation_limit_hit: bool,
}

pub(super) enum SourceScan {
    WithoutExtractor,
    Oversize,
    Unreadable,
    Observations(Vec<Observation>),
}

#[allow(clippy::struct_excessive_bools)]
#[derive(Debug, Clone, Copy, Default)]
pub(super) struct ProviderHints {
    pub(super) kafka: bool,
    pub(super) amqp: bool,
    pub(super) rabbitmq: bool,
    pub(super) jms: bool,
    pub(super) nats: bool,
    pub(super) sqs: bool,
    pub(super) sns: bool,
}

#[derive(Debug, Default)]
pub(super) struct Bindings {
    pub(super) resources: BTreeMap<String, (Entity, String)>,
    pub(super) consumer_groups: BTreeMap<String, String>,
    pub(super) transports: BTreeMap<String, BTreeSet<Transport>>,
    pub(super) aliases: BTreeMap<String, String>,
}

#[derive(Debug)]
pub(super) struct Call<'tokens, 'source> {
    pub(super) name: String,
    pub(super) chain: String,
    pub(super) receiver: Option<String>,
    pub(super) args: &'tokens [Token],
    pub(super) line: u32,
    pub(super) column: u32,
    pub(super) evidence: String,
    pub(super) source: &'source str,
}

#[derive(Debug)]
pub(super) struct Detection {
    pub(super) transport: Transport,
    pub(super) entity: Entity,
    pub(super) role: Role,
    pub(super) resources: Vec<Option<String>>,
    pub(super) exchange: Option<String>,
    pub(super) routing_key: Option<String>,
    pub(super) consumer_group: Option<String>,
    pub(super) certainty: Certainty,
    pub(super) uncertainty: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::{Certainty, Entity, Observation, Role, Transport};
    use std::collections::BTreeSet;

    #[test]
    fn transport_labels_and_observation_keys_are_stable() {
        assert_eq!(Transport::Kafka.as_str(), "kafka");
        assert_eq!(Transport::Amqp.as_str(), "amqp");
        assert_eq!(Transport::RabbitMq.as_str(), "rabbitmq");
        assert_eq!(Transport::Jms.as_str(), "jms");
        assert_eq!(Transport::Nats.as_str(), "nats");
        assert_eq!(Transport::Sqs.as_str(), "sqs");
        assert_eq!(Transport::Sns.as_str(), "sns");
        assert_eq!(Transport::RabbitMq.contract_family(), Transport::Amqp);
        assert_eq!(Transport::Jms.contract_family(), Transport::Jms);
        assert_eq!(Entity::Topic.as_str(), "topic");
        assert_eq!(Entity::Queue.as_str(), "queue");
        assert_eq!(Entity::Exchange.as_str(), "exchange");
        assert_eq!(Entity::Subject.as_str(), "subject");
        assert_eq!(Entity::Destination.as_str(), "destination");
        assert_eq!(Entity::Binding.as_str(), "binding");
        assert_eq!(Role::Producer.as_str(), "producer");
        assert_eq!(Role::Consumer.as_str(), "consumer");
        assert_eq!(Role::Declare.as_str(), "declare");
        assert_eq!(Role::Bind.as_str(), "bind");
        assert_eq!(Certainty::Exact.as_str(), "exact");
        assert_eq!(Certainty::Derived.as_str(), "derived");
        assert_eq!(Certainty::Ambiguous.as_str(), "ambiguous");
        let observation = Observation {
            repository: "repo".into(),
            path: "src/app.rs".into(),
            line: 1,
            column: 1,
            language: "rust".into(),
            transport: Transport::Jms,
            entity: Entity::Queue,
            role: Role::Producer,
            resource: Some("orders".into()),
            exchange: None,
            routing_key: None,
            consumer_group: None,
            receiver: None,
            evidence: "createProducer".into(),
            origin: "test",
            certainty: Certainty::Exact,
            uncertainty: None,
            candidates: BTreeSet::from([Transport::Jms]),
            runtime_observed: false,
        };
        let key = observation.key().expect("named resource");
        assert_eq!(key.transport, Transport::Jms);
        assert_eq!(key.resource, "orders");
        let json = observation.to_json();
        assert_eq!(json["transport"].as_str(), Some("jms"));
        let unnamed = Observation {
            resource: None,
            ..observation
        };
        assert!(unnamed.key().is_none());
    }
}
