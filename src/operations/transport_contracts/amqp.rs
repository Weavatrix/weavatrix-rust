use super::{
    Bindings, Call, Certainty, Detection, Entity, ProviderHints, Role, Transport, first_value,
    non_empty_resources, positional_values, property, resource_values,
};

pub(super) fn detect_amqp(
    call: &Call<'_, '_>,
    name: &str,
    chain: &str,
    hints: ProviderHints,
    bindings: &Bindings,
) -> Option<Detection> {
    if !is_amqp_call(name, chain, hints) {
        return None;
    }
    let positional = |index| positional_values(call, index, bindings);
    let detection = match name {
        "assertqueue" | "queuedeclare" | "queue_declare" | "createqueue" => Detection {
            transport: Transport::Amqp,
            entity: Entity::Queue,
            role: Role::Declare,
            resources: non_empty_resources(resource_values(call, &["queue", "name"], 0, bindings)),
            exchange: None,
            routing_key: None,
            consumer_group: None,
            certainty: Certainty::Exact,
            uncertainty: None,
        },
        "assertexchange" | "exchangedeclare" | "exchange_declare" => Detection {
            transport: Transport::Amqp,
            entity: Entity::Exchange,
            role: Role::Declare,
            resources: non_empty_resources(resource_values(
                call,
                &["exchange", "name"],
                0,
                bindings,
            )),
            exchange: None,
            routing_key: None,
            consumer_group: None,
            certainty: Certainty::Exact,
            uncertainty: None,
        },
        "bindqueue" | "queuebind" | "queue_bind" => binding_detection(call, bindings),
        "sendtoqueue" | "send_to_queue" => Detection {
            transport: Transport::Amqp,
            entity: Entity::Queue,
            role: Role::Producer,
            resources: non_empty_resources(positional(0)),
            exchange: None,
            routing_key: None,
            consumer_group: None,
            certainty: Certainty::Exact,
            uncertainty: None,
        },
        "consume" | "basicconsume" | "basic_consume" | "rabbitlistener" => Detection {
            transport: Transport::Amqp,
            entity: Entity::Queue,
            role: Role::Consumer,
            resources: non_empty_resources(resource_values(
                call,
                &["queue", "queues"],
                0,
                bindings,
            )),
            exchange: None,
            routing_key: None,
            consumer_group: property(call, &["consumertag", "consumer_tag"]),
            certainty: Certainty::Exact,
            uncertainty: None,
        },
        "publish" | "basicpublish" | "basic_publish" | "publishwithcontext" | "convertandsend" => {
            let exchange = first_value(call, &["exchange"], 0, bindings);
            let routing_key = first_value(call, &["routingkey", "routing_key"], 1, bindings);
            Detection {
                transport: Transport::Amqp,
                entity: Entity::Exchange,
                role: Role::Producer,
                resources: vec![exchange.clone()],
                exchange,
                routing_key,
                consumer_group: None,
                certainty: Certainty::Exact,
                uncertainty: None,
            }
        }
        _ => return None,
    };
    Some(detection)
}

fn is_amqp_call(name: &str, chain: &str, hints: ProviderHints) -> bool {
    const EXPLICIT_CALLS: &[&str] = &[
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
    ];
    hints.amqp || hints.rabbitmq || EXPLICIT_CALLS.contains(&name) || chain.contains("amqp")
}

fn binding_detection(call: &Call<'_, '_>, bindings: &Bindings) -> Detection {
    let queue = first_value(call, &["queue"], 0, bindings);
    let go_order = call.name == "QueueBind";
    let exchange = first_value(call, &["exchange"], if go_order { 2 } else { 1 }, bindings);
    let routing_key = first_value(
        call,
        &["routingkey", "routing_key", "bindingkey"],
        if go_order { 1 } else { 2 },
        bindings,
    );
    Detection {
        transport: Transport::Amqp,
        entity: Entity::Binding,
        role: Role::Bind,
        resources: vec![queue],
        exchange,
        routing_key,
        consumer_group: None,
        certainty: Certainty::Exact,
        uncertainty: None,
    }
}
