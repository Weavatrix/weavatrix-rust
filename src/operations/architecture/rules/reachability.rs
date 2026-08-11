use super::super::contract::{component_for, list_contains};
use super::{rule_selects_edge, stable_hash};
use crate::engine::RepositoryState;
use crate::operations::node_path;
use blazingly_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet, VecDeque};
use weavatrix_graph::{EdgeIndex, GraphView, NodeIndex};

struct PolicyPath {
    nodes: Vec<NodeIndex>,
    edges: Vec<EdgeIndex>,
}

pub(super) fn violations(state: &RepositoryState, contract: &Value) -> Vec<Value> {
    let mut output = BTreeMap::new();
    for rule in contract
        .get("dependencyRules")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        let action = rule["action"].as_str().unwrap_or_default();
        let reachability = rule
            .get("reachability")
            .and_then(Value::as_str)
            .unwrap_or("direct");
        if action == "require" {
            required_violations(state, contract, rule, reachability, &mut output);
        } else if action == "forbid" && reachability == "transitive" {
            forbidden_violations(state, contract, rule, &mut output);
        }
    }
    output.into_values().collect()
}

fn required_violations(
    state: &RepositoryState,
    contract: &Value,
    rule: &Value,
    reachability: &str,
    output: &mut BTreeMap<String, Value>,
) {
    let max_hops = (reachability == "direct").then_some(1);
    for (source_file, seeds) in source_nodes(state, contract, rule) {
        if shortest_path(state, contract, rule, &seeds, max_hops).is_some() {
            continue;
        }
        let identity = format!(
            "{}|required|{source_file}",
            rule["id"].as_str().unwrap_or("rule")
        );
        let fingerprint = stable_hash(&identity);
        let component = component_for(contract, &source_file).unwrap_or_default();
        output.insert(
            fingerprint.clone(),
            json!({
                "fingerprint": fingerprint,
                "category": "requirement",
                "rule": rule,
                "source": {"file": source_file, "component": component},
                "evidence": {
                    "kind": "missing_required_dependency",
                    "target_components": rule["to"]
                }
            }),
        );
    }
}

fn forbidden_violations(
    state: &RepositoryState,
    contract: &Value,
    rule: &Value,
    output: &mut BTreeMap<String, Value>,
) {
    for (source_file, seeds) in source_nodes(state, contract, rule) {
        let Some(path) = shortest_path(state, contract, rule, &seeds, None) else {
            continue;
        };
        let path_files = path_files(state, &path.nodes);
        let Some(target_file) = path_files.last() else {
            continue;
        };
        let identity = format!(
            "{}|transitive_forbid|{source_file}|{target_file}",
            rule["id"].as_str().unwrap_or("rule")
        );
        let fingerprint = stable_hash(&identity);
        let source_component = component_for(contract, &source_file).unwrap_or_default();
        let target_component = component_for(contract, target_file).unwrap_or_default();
        let relations = path
            .edges
            .iter()
            .filter_map(|index| state.graph().edge_at(*index))
            .map(|edge| edge.kind.as_str())
            .collect::<Vec<_>>();
        output.insert(
            fingerprint.clone(),
            json!({
                "fingerprint": fingerprint,
                "category": "dependency",
                "rule": rule,
                "source": {"file": source_file, "component": source_component},
                "target": {"file": target_file, "component": target_component},
                "evidence": {
                    "kind": "transitive_dependency",
                    "path_files": path_files,
                    "relations": relations,
                    "hops": path.edges.len()
                }
            }),
        );
    }
}

fn source_nodes(
    state: &RepositoryState,
    contract: &Value,
    rule: &Value,
) -> BTreeMap<String, Vec<NodeIndex>> {
    let mut sources = BTreeMap::<String, Vec<NodeIndex>>::new();
    for (slot, node) in state.graph().nodes().iter().enumerate() {
        let Some(path) = node_path(node) else {
            continue;
        };
        let Some(component) = component_for(contract, path) else {
            continue;
        };
        if list_contains(rule.get("from"), component) {
            let index = NodeIndex::new(u32::try_from(slot).unwrap_or(u32::MAX));
            sources.entry(path.to_owned()).or_default().push(index);
        }
    }
    sources
}

fn shortest_path(
    state: &RepositoryState,
    contract: &Value,
    rule: &Value,
    seeds: &[NodeIndex],
    max_hops: Option<usize>,
) -> Option<PolicyPath> {
    let mut queue = VecDeque::new();
    let mut seen = BTreeSet::new();
    let mut previous = BTreeMap::<NodeIndex, (NodeIndex, EdgeIndex)>::new();
    for seed in seeds.iter().copied() {
        if seen.insert(seed) {
            queue.push_back((seed, 0));
        }
    }
    while let Some((node, depth)) = queue.pop_front() {
        if max_hops.is_some_and(|limit| depth >= limit) {
            continue;
        }
        for edge_index in state.graph().outgoing_edges(node) {
            let Some(edge) = state.graph().edge_at(edge_index) else {
                continue;
            };
            if !rule_selects_edge(rule, edge) {
                continue;
            }
            let Some(endpoints) = state.graph().edge_endpoints(edge_index) else {
                continue;
            };
            let target = endpoints.target();
            if !seen.insert(target) {
                continue;
            }
            previous.insert(target, (node, edge_index));
            if is_target(state, contract, rule, target) {
                return Some(reconstruct(target, &previous));
            }
            queue.push_back((target, depth + 1));
        }
    }
    None
}

fn is_target(state: &RepositoryState, contract: &Value, rule: &Value, index: NodeIndex) -> bool {
    state
        .graph()
        .node_at(index)
        .and_then(node_path)
        .and_then(|path| component_for(contract, path))
        .is_some_and(|component| list_contains(rule.get("to"), component))
}

fn reconstruct(
    mut cursor: NodeIndex,
    previous: &BTreeMap<NodeIndex, (NodeIndex, EdgeIndex)>,
) -> PolicyPath {
    let mut nodes = vec![cursor];
    let mut edges = Vec::new();
    while let Some((parent, edge)) = previous.get(&cursor).copied() {
        edges.push(edge);
        nodes.push(parent);
        cursor = parent;
    }
    nodes.reverse();
    edges.reverse();
    PolicyPath { nodes, edges }
}

fn path_files(state: &RepositoryState, nodes: &[NodeIndex]) -> Vec<String> {
    let mut files = Vec::new();
    for path in nodes
        .iter()
        .filter_map(|index| state.graph().node_at(*index))
        .filter_map(node_path)
    {
        if files.last().is_none_or(|previous| previous != path) {
            files.push(path.to_owned());
        }
    }
    files
}
