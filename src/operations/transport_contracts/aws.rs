use super::{
    Bindings, Call, Certainty, Detection, Entity, ProviderHints, Role, Transport, bindings_none,
    non_empty_resources, positional_identifier, property, resource_values,
};

pub(super) fn detect_aws(
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

pub(super) fn destination_entity(call: &Call<'_, '_>, bindings: &Bindings) -> Entity {
    let Some(identifier) = positional_identifier(call, 0) else {
        return Entity::Destination;
    };
    bindings
        .resources
        .get(&identifier)
        .map_or(Entity::Destination, |(entity, _)| *entity)
}
