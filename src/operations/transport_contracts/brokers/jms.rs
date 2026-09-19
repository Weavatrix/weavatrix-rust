use super::{
    Bindings, Call, Certainty, Detection, Entity, ProviderHints, Role, Transport,
    destination_entity, non_empty_resources, positional_values, property, resource_values,
};

pub(super) fn detect_jms(
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
