use super::components::Component;
use crate::engine::RepositoryState;
use crate::operations::graph::coupling_relations;
use crate::operations::node_path;
use blazingly_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};

const MAX_SAMPLE: usize = 3;

pub(super) fn collect(state: &RepositoryState, components: &[Component]) -> Vec<Value> {
    let allowed = coupling_relations();
    let owner = file_owners(components);
    let mut counts = BTreeMap::<
        (String, String, String),
        (Vec<usize>, BTreeSet<(String, String)>, Vec<Value>),
    >::new();
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
        if from == to {
            continue;
        }
        let entry = counts
            .entry((from.clone(), to.clone(), relation.to_owned()))
            .or_default();
        entry.0.push(edge_index);
        entry.1.insert((from_file.clone(), to_file.clone()));
        if entry.2.len() < MAX_SAMPLE {
            entry
                .2
                .push(json!({"source_file": from_file, "target_file": to_file,
                "base_edge_index": edge_index, "source_node": edge.source,
                "target_node": edge.target, "provenance": edge.provenance}));
        }
    }
    counts
        .into_iter()
        .map(|((from, to, relation), (base_edge_indices, file_pairs, evidence_sample))| {
            json!({
                "from": from,
                "to": to,
                "relation": relation,
                "count": base_edge_indices.len(),
                "file_pairs": file_pairs.len(),
                "base_edge_indices": base_edge_indices,
                "evidence_sample": evidence_sample,
                "sample": file_pairs.iter().take(MAX_SAMPLE).map(|(from, to)| format!("{from} -> {to}")).collect::<Vec<_>>()
            })
        })
        .collect()
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
