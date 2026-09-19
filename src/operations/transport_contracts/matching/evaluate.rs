use super::{
    BTreeMap, BTreeSet, Certainty, ContractKey, Entity, Observation, Role, Transport, Value, json,
};

#[derive(Default)]
pub(super) struct Evaluation {
    pub(super) contracts: Vec<Value>,
    pub(super) matches: Vec<Value>,
    pub(super) mismatches: Vec<Value>,
    pub(super) ambiguities: Vec<Value>,
}

pub(super) fn evaluate(
    observations: &[Observation],
    max_contracts: usize,
    per_contract: usize,
) -> Evaluation {
    let grouped = group_observations(observations);
    let bindings = observations
        .iter()
        .filter(|observation| {
            matches!(observation.transport, Transport::Amqp | Transport::RabbitMq)
        })
        .filter(|observation| observation.role == Role::Bind)
        .collect::<Vec<_>>();
    let mut evaluation = Evaluation::default();
    for (key, evidence) in grouped.into_iter().take(max_contracts) {
        evaluate_contract(
            &key,
            &evidence,
            &bindings,
            observations,
            per_contract,
            &mut evaluation,
        );
    }
    add_cross_kind_mismatches(observations, &mut evaluation.mismatches);
    evaluation
}

fn group_observations(observations: &[Observation]) -> BTreeMap<ContractKey, Vec<&Observation>> {
    let mut grouped: BTreeMap<ContractKey, Vec<&Observation>> = BTreeMap::new();
    for observation in observations {
        if observation.certainty != Certainty::Ambiguous
            && let Some(key) = observation.key()
        {
            grouped.entry(key).or_default().push(observation);
        }
    }
    grouped
}

fn evaluate_contract<'a>(
    key: &ContractKey,
    evidence: &[&'a Observation],
    bindings: &[&'a Observation],
    observations: &'a [Observation],
    per_contract: usize,
    evaluation: &mut Evaluation,
) {
    let producers = evidence
        .iter()
        .copied()
        .filter(|item| item.role == Role::Producer)
        .collect::<Vec<_>>();
    let mut consumers = evidence
        .iter()
        .copied()
        .filter(|item| item.role == Role::Consumer)
        .collect::<Vec<_>>();
    let declarations = evidence
        .iter()
        .copied()
        .filter(|item| matches!(item.role, Role::Declare | Role::Bind))
        .collect::<Vec<_>>();
    let routed = route_amqp_consumers(
        key,
        &producers,
        bindings,
        observations,
        &mut consumers,
        &mut evaluation.mismatches,
    );
    consumers.sort();
    consumers.dedup();

    let providers = evidence
        .iter()
        .map(|item| item.transport)
        .collect::<BTreeSet<_>>();
    let provider = providers
        .first()
        .filter(|_| providers.len() == 1)
        .copied()
        .unwrap_or(key.transport)
        .as_str();
    let paired = !producers.is_empty() && !consumers.is_empty();
    let verdict = match (paired, producers.is_empty(), consumers.is_empty()) {
        (true, _, _) => "MATCH",
        (false, true, true) => "CONFIGURATION_ONLY",
        _ => "UNPAIRED_STATIC_EVIDENCE",
    };
    let key_text = format!(
        "{}:{}:{}",
        key.transport.as_str(),
        key.entity.as_str(),
        key.resource
    );
    if paired {
        evaluation.matches.push(json!({
            "key": key_text,
            "producer_count": producers.len(),
            "consumer_count": consumers.len(),
            "routed_matches": routed
        }));
    } else if !producers.is_empty() || !consumers.is_empty() {
        evaluation.ambiguities.push(json!({
            "code": "UNPAIRED_STATIC_EVIDENCE",
            "key": key_text,
            "candidates": ["external_peer", "runtime_configuration", "repository_outside_scope"],
            "evidence": evidence.iter().take(per_contract).map(|item| item.to_json()).collect::<Vec<_>>()
        }));
    }
    evaluation.contracts.push(json!({
        "key": key_text,
        "transport": "event",
        "provider": provider,
        "contract_family": key.transport.as_str(),
        "provider_candidates": providers.iter().map(|provider| provider.as_str()).collect::<Vec<_>>(),
        "entity": key.entity.as_str(),
        "kind": key.entity.as_str(),
        "name": key.resource,
        "verdict": verdict,
        "matched": paired,
        "backend_contract": evidence.iter()
            .find(|item| item.repository == "backend")
            .map(|item| item.to_json()),
        "callsites": evidence.iter()
            .filter(|item| item.repository != "backend")
            .take(per_contract)
            .map(|item| item.to_json())
            .collect::<Vec<_>>(),
        "producers": producers.into_iter().take(per_contract).map(Observation::to_json).collect::<Vec<_>>(),
        "consumers": consumers.into_iter().take(per_contract).map(Observation::to_json).collect::<Vec<_>>(),
        "configuration": declarations.into_iter().take(per_contract).map(Observation::to_json).collect::<Vec<_>>()
    }));
}

fn route_amqp_consumers<'a>(
    key: &ContractKey,
    producers: &[&'a Observation],
    bindings: &[&'a Observation],
    observations: &'a [Observation],
    consumers: &mut Vec<&'a Observation>,
    mismatches: &mut Vec<Value>,
) -> Vec<Value> {
    let mut routed = Vec::new();
    if key.transport != Transport::Amqp || key.entity != Entity::Exchange {
        return routed;
    }
    for producer in producers {
        for binding in bindings {
            if binding.exchange.as_deref() != Some(key.resource.as_str()) {
                continue;
            }
            let compatible = routing_keys_match(
                producer.routing_key.as_deref(),
                binding.routing_key.as_deref(),
            );
            let queue = binding.resource.as_deref().unwrap_or_default();
            for candidate in observations.iter().filter(|candidate| {
                matches!(candidate.transport, Transport::Amqp | Transport::RabbitMq)
                    && candidate.entity == Entity::Queue
                    && candidate.role == Role::Consumer
                    && candidate.resource.as_deref() == Some(queue)
            }) {
                if compatible {
                    consumers.push(candidate);
                    routed.push(json!({
                        "producer": producer.to_json(),
                        "binding": binding.to_json(),
                        "consumer": candidate.to_json(),
                        "routing": "COMPATIBLE"
                    }));
                } else {
                    mismatches.push(json!({
                        "code": "AMQP_ROUTING_KEY_MISMATCH",
                        "exchange": key.resource,
                        "producer": producer.to_json(),
                        "binding": binding.to_json(),
                        "consumer": candidate.to_json()
                    }));
                }
            }
        }
    }
    routed
}

pub(super) fn add_cross_kind_mismatches(observations: &[Observation], mismatches: &mut Vec<Value>) {
    let producers = observations
        .iter()
        .filter(|item| item.role == Role::Producer)
        .filter_map(|item| item.resource.as_deref().map(|resource| (item, resource)));
    for (producer, resource) in producers {
        for consumer in observations.iter().filter(|item| {
            item.role == Role::Consumer && item.resource.as_deref() == Some(resource)
        }) {
            if producer.transport.contract_family() != consumer.transport.contract_family() {
                mismatches.push(json!({
                    "code": "TRANSPORT_MISMATCH",
                    "resource": resource,
                    "producer": producer.to_json(),
                    "consumer": consumer.to_json()
                }));
            } else if producer.transport.contract_family() == consumer.transport.contract_family()
                && producer.entity != consumer.entity
                && !matches!(
                    (
                        producer.transport.contract_family(),
                        producer.entity,
                        consumer.entity
                    ),
                    (Transport::Jms, Entity::Destination, _)
                        | (Transport::Jms, _, Entity::Destination)
                )
            {
                mismatches.push(json!({
                    "code": "DESTINATION_KIND_MISMATCH",
                    "resource": resource,
                    "producer": producer.to_json(),
                    "consumer": consumer.to_json()
                }));
            }
        }
    }
    mismatches.sort_by(|left, right| {
        left["code"]
            .as_str()
            .unwrap_or_default()
            .cmp(right["code"].as_str().unwrap_or_default())
            .then_with(|| {
                left["resource"]
                    .as_str()
                    .unwrap_or_default()
                    .cmp(right["resource"].as_str().unwrap_or_default())
            })
    });
    mismatches.dedup();
}

pub(super) fn routing_keys_match(producer: Option<&str>, binding: Option<&str>) -> bool {
    let Some(producer) = producer else {
        return false;
    };
    let Some(binding) = binding else {
        return false;
    };
    if producer == binding {
        return true;
    }
    let producer = producer.split('.').collect::<Vec<_>>();
    let binding = binding.split('.').collect::<Vec<_>>();
    let mut left = 0_usize;
    let mut right = 0_usize;
    while left < producer.len() && right < binding.len() {
        match binding[right] {
            "#" => return true,
            "*" => {
                left += 1;
                right += 1;
            }
            exact if exact == producer[left] => {
                left += 1;
                right += 1;
            }
            _ => return false,
        }
    }
    (left == producer.len() && right == binding.len())
        || (right + 1 == binding.len() && binding[right] == "#")
}
