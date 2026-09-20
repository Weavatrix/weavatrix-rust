use super::components::Component;
use crate::engine::RepositoryState;
use crate::operations::graph::coupling_relations;
use crate::operations::node_path;
use blazingly_json::{Value, json};
use std::collections::BTreeMap;

const MAX_EDGES: usize = 200;
const MAX_SAMPLE: usize = 3;

pub(super) fn collect(state: &RepositoryState, components: &[Component]) -> Vec<Value> {
    let allowed = coupling_relations();
    let owner = file_owners(components);
    let mut counts = BTreeMap::<(String, String, String), (u64, Vec<String>)>::new();
    for edge in state.graph().edges() {
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
        entry.0 = entry.0.saturating_add(1);
        if entry.1.len() < MAX_SAMPLE {
            entry.1.push(format!("{from_file} -> {to_file}"));
        }
    }
    counts
        .into_iter()
        .take(MAX_EDGES)
        .map(|((from, to, relation), (count, sample))| {
            json!({
                "from": from,
                "to": to,
                "relation": relation,
                "count": count,
                "sample": sample
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
