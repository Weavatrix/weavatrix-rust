use super::components::Component;
use crate::engine::RepositoryState;
use crate::operations::graph::coupling_relations;
use crate::operations::node_path;
use blazingly_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};

const MAX_SAMPLE: usize = 3;

pub(super) struct Collection {
    pub cross: Vec<Value>,
    pub internal: Vec<Value>,
}

#[derive(Default)]
struct Aggregate {
    base_edge_indices: Vec<usize>,
    file_pairs: BTreeSet<(String, String)>,
    evidence_sample: Vec<Value>,
    evidence_histogram: BTreeMap<String, usize>,
}

pub(super) fn collect(state: &RepositoryState, components: &[Component]) -> Collection {
    let allowed = coupling_relations();
    let owner = file_owners(components);
    let mut cross = BTreeMap::<(String, String, String), Aggregate>::new();
    let mut internal = BTreeMap::<(String, String), Aggregate>::new();
    for (edge_index, edge) in state.graph().edges().iter().enumerate() {
        let relation = edge.kind.as_str();
        if !allowed.contains(relation) {
            continue;
        }
        let Some(from_file) = state.graph().node(edge.source.as_str()).and_then(node_path) else {
            continue;
        };
        let Some(to_file) = state.graph().node(edge.target.as_str()).and_then(node_path) else {
            continue;
        };
        let from_file = from_file.replace('\\', "/");
        let to_file = to_file.replace('\\', "/");
        let Some(from) = owner.get(&from_file) else {
            continue;
        };
        let Some(to) = owner.get(&to_file) else {
            continue;
        };
        let entry = if from == to {
            internal
                .entry((from.clone(), relation.to_owned()))
                .or_default()
        } else {
            cross
                .entry((from.clone(), to.clone(), relation.to_owned()))
                .or_default()
        };
        record(entry, edge_index, &from_file, &to_file, edge);
    }
    Collection {
        cross: cross
            .into_iter()
            .map(|((from, to, relation), aggregate)| {
                aggregate_json(&from, Some(&to), &relation, &aggregate)
            })
            .collect(),
        internal: internal
            .into_iter()
            .map(|((component, relation), aggregate)| {
                aggregate_json(&component, None, &relation, &aggregate)
            })
            .collect(),
    }
}

fn record(
    aggregate: &mut Aggregate,
    edge_index: usize,
    from_file: &str,
    to_file: &str,
    edge: &weavatrix_graph::Edge,
) {
    aggregate.base_edge_indices.push(edge_index);
    aggregate
        .file_pairs
        .insert((from_file.to_owned(), to_file.to_owned()));
    *aggregate
        .evidence_histogram
        .entry(edge.provenance.evidence.as_str().to_owned())
        .or_default() += 1;
    if aggregate.evidence_sample.len() < MAX_SAMPLE {
        aggregate.evidence_sample.push(json!({
            "source_file": from_file, "target_file": to_file,
            "base_edge_index": edge_index, "source_node": edge.source,
            "target_node": edge.target, "provenance": edge.provenance
        }));
    }
}

fn aggregate_json(from: &str, to: Option<&str>, relation: &str, aggregate: &Aggregate) -> Value {
    json!({
        "from": from,
        "to": to,
        "component": to.is_none().then_some(from),
        "relation": relation,
        "occurrence_count": aggregate.base_edge_indices.len(),
        "distinct_base_edges": aggregate.base_edge_indices.len(),
        "distinct_file_pairs": aggregate.file_pairs.len(),
        "count": aggregate.base_edge_indices.len(),
        "file_pairs": aggregate.file_pairs.len(),
        "base_edge_indices": aggregate.base_edge_indices,
        "evidence_histogram": aggregate.evidence_histogram,
        "condition_groups": {"UNSPECIFIED": aggregate.file_pairs.len()},
        "evidence_sample": aggregate.evidence_sample,
        "sample": aggregate.file_pairs.iter().take(MAX_SAMPLE)
            .map(|(source, target)| format!("{source} -> {target}")).collect::<Vec<_>>()
    })
}

fn file_owners(components: &[Component]) -> BTreeMap<String, String> {
    components
        .iter()
        .flat_map(|component| {
            component
                .files
                .iter()
                .map(|file| (file.clone(), component.id.clone()))
        })
        .collect()
}
