use super::{
    Call, Certainty, Detection, Entity, ProviderHints, Role, Transport, bindings_none,
    non_empty_resources, property, resource_values,
};

pub(super) fn detect_nats(
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
