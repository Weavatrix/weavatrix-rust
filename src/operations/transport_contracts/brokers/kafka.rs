use super::{
    Call, Certainty, Detection, Entity, ProviderHints, Role, Transport, bindings_none,
    non_empty_resources, positional_values, property, resource_values,
};

pub(super) fn detect_kafka(
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
