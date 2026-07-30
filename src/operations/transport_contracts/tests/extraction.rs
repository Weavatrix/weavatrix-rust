use super::*;

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
    let observations = extract_observations("repo", "src/events.ts", Language::TypeScript, source);
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
    let observations = extract_observations("repo", "aws-events.ts", Language::TypeScript, source);
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
    let source =
        "import nats\nawait nats.publish(subject_name, payload)\nawait nats.subscribe(subject())";
    let observations = extract_observations("repo", "worker.py", Language::Python, source);
    assert_eq!(observations.len(), 2);
    assert!(observations.iter().all(|item| {
        item.resource.is_none()
            && item.certainty == Certainty::Ambiguous
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
        item.certainty == Certainty::Ambiguous
            && item
                .uncertainty
                .as_deref()
                .is_some_and(|reason| reason.contains("multiple concrete transports"))
    }));
    assert!(union.iter().all(|item| {
        item.candidates == std::collections::BTreeSet::from([Transport::RabbitMq, Transport::Nats])
    }));
    assert_eq!(
        union
            .iter()
            .map(|item| item.transport)
            .collect::<std::collections::BTreeSet<_>>(),
        std::collections::BTreeSet::from([Transport::RabbitMq, Transport::Nats])
    );
}
