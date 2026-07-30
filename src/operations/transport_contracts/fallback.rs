use super::{
    BTreeSet, Certainty, EdgeKind, Entity, NodeKind, Observation, RepositoryState, Role, Transport,
    Value, language_for_path,
};

pub(super) fn add_graph_fallbacks(
    repository: &str,
    state: &RepositoryState,
    args: &Value,
    source_keys: &BTreeSet<(String, String, Role, String, u32)>,
    max_observations: usize,
    observations: &mut Vec<Observation>,
) {
    for (slot, node) in state.graph().nodes().iter().enumerate() {
        let entity = match node.kind {
            NodeKind::Topic => Entity::Topic,
            NodeKind::Queue => Entity::Queue,
            NodeKind::Exchange => Entity::Exchange,
            NodeKind::Binding => Entity::Binding,
            _ => continue,
        };
        if !super::super::node_is_visible(state, slot, args) {
            continue;
        }
        let index = weavatrix_graph::NodeIndex::new(u32::try_from(slot).unwrap_or(u32::MAX));
        for edge in state.graph().incoming_at(index) {
            let role = match edge.kind {
                EdgeKind::Publishes => Role::Producer,
                EdgeKind::Consumes => Role::Consumer,
                EdgeKind::Binds => Role::Bind,
                EdgeKind::Configures => Role::Declare,
                _ => continue,
            };
            let span = edge.provenance.span.as_ref();
            let path = span.map(|span| span.file.clone()).unwrap_or_default();
            let Some(language) = language_for_path(&path) else {
                continue;
            };
            let line = span.map_or(0, |span| span.start.line);
            if source_keys.contains(&(
                repository.to_owned(),
                path.clone(),
                role,
                node.label.clone(),
                line,
            )) {
                continue;
            }
            let candidates = fallback_transport_candidates(entity);
            for transport in &candidates {
                let ambiguous = candidates.len() > 1;
                observations.push(Observation {
                    repository: repository.to_owned(),
                    path: path.clone(),
                    line,
                    column: span.map_or(0, |span| span.start.column),
                    language: language.as_str().to_owned(),
                    transport: *transport,
                    entity,
                    role,
                    resource: Some(node.label.clone()),
                    exchange: None,
                    routing_key: None,
                    consumer_group: None,
                    receiver: None,
                    evidence: edge.provenance.detail.clone().unwrap_or_default(),
                    origin: "graph_domain",
                    certainty: if ambiguous {
                        Certainty::Ambiguous
                    } else {
                        Certainty::Derived
                    },
                    uncertainty: ambiguous.then(|| {
                        "graph-domain evidence identifies the destination kind but has multiple concrete transport candidates"
                            .to_owned()
                    }),
                    candidates: candidates.clone(),
                    runtime_observed: false,
                });
                if observations.len() >= max_observations {
                    return;
                }
            }
        }
    }
}

pub(super) fn fallback_transport_candidates(entity: Entity) -> BTreeSet<Transport> {
    match entity {
        Entity::Topic => BTreeSet::from([Transport::Kafka, Transport::Jms, Transport::Sns]),
        Entity::Queue => BTreeSet::from([
            Transport::Amqp,
            Transport::RabbitMq,
            Transport::Jms,
            Transport::Sqs,
        ]),
        Entity::Exchange | Entity::Binding => {
            BTreeSet::from([Transport::Amqp, Transport::RabbitMq])
        }
        Entity::Subject => BTreeSet::from([Transport::Nats]),
        Entity::Destination => BTreeSet::from([Transport::Jms]),
    }
}

pub(super) fn observation_identity(
    observation: &Observation,
) -> Option<(String, String, Role, String, u32)> {
    observation.resource.as_ref().map(|resource| {
        (
            observation.repository.clone(),
            observation.path.clone(),
            observation.role,
            resource.clone(),
            observation.line,
        )
    })
}
