//! Shared, lossless mapping from typed parser facts into graph facts.

use super::{FileFacts, SymbolFact, SymbolLocator};
use crate::model::Diagnostic;
use std::collections::BTreeMap;
use weavatrix_graph::{EdgeKind, NodeKind, SourcePosition, SourceSpan};
use weavatrix_parse::{ParseDiagnostic, Span};

pub(crate) fn may_contain_transport_marker(source: &str) -> bool {
    const MARKERS: &[&str] = &[
        "kafka",
        "kafkajs",
        "rdkafka",
        "sarama",
        "confluent_kafka",
        "amqp",
        "rabbitmq",
        "rabbit",
        "pika",
        "lapin",
        "kombu",
        "jms",
        "activemq",
        "artemis",
        "nats",
        "sqs",
        "sns",
        "producerrecord",
        "futurerecord",
        "baserecord",
        "consumepartition",
        "subscribetopics",
        "addconsumetopics",
        "newwriter",
        "newreader",
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
        "jmslistener",
        "createqueue",
        "createtopic",
        "createproducer",
        "createconsumer",
        "convertandsend",
        "queueurl",
        "queue_url",
        "topicarn",
        "topic_arn",
        "sendmessage",
        "send_message",
        "receivemessage",
        "receive_message",
        "urlsession",
        "websocket",
        "websockettask",
        "wcsession",
        "watchconnectivity",
    ];
    let lowercase = source.to_ascii_lowercase();
    MARKERS.iter().any(|marker| lowercase.contains(marker))
}

pub(crate) fn file_facts_have_transport_evidence(facts: &FileFacts) -> bool {
    facts.imports.iter().chain(&facts.reexports).any(|import| {
        may_contain_transport_marker(&import.target)
            || import.bindings.iter().any(|binding| {
                may_contain_transport_marker(&binding.imported)
                    || may_contain_transport_marker(&binding.local)
            })
    }) || facts.references.iter().any(|reference| {
        reference.kind == EdgeKind::Calls
            && (is_transport_call_name(&reference.name)
                || reference
                    .receiver
                    .as_deref()
                    .is_some_and(may_contain_transport_marker))
    }) || facts.domains.iter().any(|domain| {
        matches!(
            domain.kind,
            NodeKind::Topic | NodeKind::Queue | NodeKind::Exchange
        )
    })
}

fn is_transport_call_name(name: &str) -> bool {
    matches!(
        name.to_ascii_lowercase().as_str(),
        "kafkalistener"
            | "rabbitlistener"
            | "jmslistener"
            | "producerrecord"
            | "futurerecord"
            | "baserecord"
            | "consumepartition"
            | "subscribetopics"
            | "addconsumetopics"
            | "newwriter"
            | "newreader"
            | "assertqueue"
            | "queuedeclare"
            | "queue_declare"
            | "assertexchange"
            | "exchangedeclare"
            | "exchange_declare"
            | "bindqueue"
            | "queuebind"
            | "queue_bind"
            | "sendtoqueue"
            | "basicpublish"
            | "basic_publish"
            | "basicconsume"
            | "basic_consume"
            | "createqueue"
            | "createtopic"
            | "createproducer"
            | "createconsumer"
            | "convertandsend"
            | "sendmessage"
            | "send_message"
            | "receivemessage"
            | "receive_message"
            | "urlsession"
            | "websockettask"
            | "wcsession"
    )
}

pub(super) fn add_symbol(
    facts: &mut FileFacts,
    owners: &mut BTreeMap<String, SymbolLocator>,
    name: String,
    kind: NodeKind,
    span: SourceSpan,
    owner: Option<String>,
) -> SymbolLocator {
    let locator = SymbolLocator {
        name: name.clone(),
        kind: kind.clone(),
        span: span.clone(),
    };
    facts.symbols.push(SymbolFact {
        name: name.clone(),
        kind,
        span,
        test_only: false,
        owner,
    });
    owners.insert(name, locator.clone());
    locator
}

pub(super) fn diagnostic(path: &str, diagnostic: &ParseDiagnostic) -> Diagnostic {
    Diagnostic {
        code: diagnostic.code.into(),
        message: diagnostic.message.clone(),
        span: Some(source_span(path, &diagnostic.span)),
    }
}

pub(super) fn facts_with_diagnostics(path: &str, diagnostics: &[ParseDiagnostic]) -> FileFacts {
    FileFacts {
        diagnostics: diagnostics
            .iter()
            .map(|diagnostic| self::diagnostic(path, diagnostic))
            .collect(),
        ..FileFacts::default()
    }
}

pub(super) fn source_span(path: &str, span: &Span) -> SourceSpan {
    SourceSpan::new(
        path,
        SourcePosition::new(span.line, span.column),
        SourcePosition::new(span.end_line, span.end_column),
    )
}

#[cfg(test)]
mod tests {
    use super::is_transport_call_name;

    #[test]
    fn transport_call_names_are_exact_api_shapes() {
        for name in [
            "KafkaListener",
            "RabbitListener",
            "JmsListener",
            "queue_declare",
            "basic_publish",
            "subscribeTopics",
            "sendMessage",
            "convertAndSend",
        ] {
            assert!(is_transport_call_name(name), "{name}");
        }
        for helper in [
            "rabbit_routing_compatible",
            "kafka_contract_summary",
            "observations",
        ] {
            assert!(!is_transport_call_name(helper), "{helper}");
        }
    }
}
