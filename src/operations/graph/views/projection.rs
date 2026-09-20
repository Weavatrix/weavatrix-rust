use super::super::coupling::coupling_relations;
use crate::engine::RepositoryState;
use std::collections::{BTreeMap, BTreeSet};
use weavatrix_graph::{Node, NodeIndex, NodeKind};

pub(super) struct Projection {
    pub clusters: Vec<Cluster>,
    pub boundaries: Vec<Boundary>,
    pub resolution: u64,
    pub hub_degree: u64,
}

pub(super) struct Cluster {
    pub key: String,
    pub kind: &'static str,
    pub files: Vec<NodeIndex>,
    pub paths: Vec<String>,
}

pub(super) struct Boundary {
    pub from: String,
    pub to: String,
    pub from_key: String,
    pub to_key: String,
    pub relation: String,
    pub why: &'static str,
}

struct Pair {
    left: String,
    right: String,
    left_dir: String,
    right_dir: String,
    relations: BTreeSet<String>,
}

pub(super) fn project(
    state: &RepositoryState,
    include_non_product: bool,
    resolution: u64,
    hub_degree: u64,
) -> Projection {
    let files = file_paths(state, include_non_product);
    let pairs = file_pairs(state, &files);
    let hubs = hub_files(&pairs, hub_degree);
    let parent = merge_directories(&pairs, &hubs, resolution);
    let clusters = cluster(&files, &hubs, &parent);
    let keys = file_keys(&clusters);
    Projection {
        clusters,
        boundaries: boundaries(&pairs, &hubs, &keys),
        resolution,
        hub_degree,
    }
}

fn file_paths(state: &RepositoryState, include_non_product: bool) -> BTreeMap<String, NodeIndex> {
    state
        .graph()
        .nodes()
        .iter()
        .enumerate()
        .filter_map(|(slot, node)| {
            if node.kind != NodeKind::File {
                return None;
            }
            if !include_non_product && crate::operations::health::is_non_product(&node.label) {
                return None;
            }
            Some((
                node.label.clone(),
                NodeIndex::new(u32::try_from(slot).unwrap_or(u32::MAX)),
            ))
        })
        .collect()
}

fn file_pairs(state: &RepositoryState, files: &BTreeMap<String, NodeIndex>) -> Vec<Pair> {
    let allowed = coupling_relations();
    let mut weights = BTreeMap::<(String, String), BTreeSet<String>>::new();
    for node in state.graph().nodes() {
        let Some(origin) = owning_file(node, files) else {
            continue;
        };
        let Some(index) = state.graph().node_index(node.id.as_str()) else {
            continue;
        };
        for edge in state.graph().outgoing_at(index) {
            if !allowed.contains(edge.kind.as_str()) {
                continue;
            }
            let Some(target) = state.graph().node(edge.target.as_str()) else {
                continue;
            };
            let Some(dest) = owning_file(target, files) else {
                continue;
            };
            if origin == dest {
                continue;
            }
            let (left, right) = ordered(&origin, &dest);
            weights
                .entry((left, right))
                .or_default()
                .insert(edge.kind.as_str().to_owned());
        }
    }
    weights
        .into_iter()
        .map(|((left, right), relations)| Pair {
            left_dir: directory(&left),
            right_dir: directory(&right),
            left,
            right,
            relations,
        })
        .collect()
}

fn owning_file(node: &Node, files: &BTreeMap<String, NodeIndex>) -> Option<String> {
    if node.kind == NodeKind::File && files.contains_key(&node.label) {
        return Some(node.label.clone());
    }
    let path = node.span.as_ref()?.file.clone();
    files.contains_key(&path).then_some(path)
}

fn hub_files(pairs: &[Pair], hub_degree: u64) -> BTreeSet<String> {
    let mut neighbors = BTreeMap::<String, BTreeSet<String>>::new();
    for pair in pairs {
        neighbors
            .entry(pair.left.clone())
            .or_default()
            .insert(pair.right.clone());
        neighbors
            .entry(pair.right.clone())
            .or_default()
            .insert(pair.left.clone());
    }
    neighbors
        .into_iter()
        .filter(|(_, adjacent)| {
            let territories = adjacent
                .iter()
                .map(|path| directory(path))
                .collect::<BTreeSet<_>>();
            adjacent.len() as u64 >= hub_degree && territories.len() >= 2
        })
        .map(|(path, _)| path)
        .collect()
}

fn merge_directories(
    pairs: &[Pair],
    hubs: &BTreeSet<String>,
    resolution: u64,
) -> BTreeMap<String, String> {
    let mut parent = BTreeMap::new();
    let mut votes = BTreeMap::<(String, String), u64>::new();
    for pair in pairs {
        if hubs.contains(&pair.left)
            || hubs.contains(&pair.right)
            || pair.left_dir == pair.right_dir
        {
            continue;
        }
        let key = ordered(&pair.left_dir, &pair.right_dir);
        *votes.entry(key).or_default() += 1;
    }
    for ((left, right), count) in votes {
        if count < resolution {
            continue;
        }
        let root_left = root(&parent, &left);
        let root_right = root(&parent, &right);
        if root_left != root_right {
            parent.insert(
                root_left.clone().max(root_right.clone()),
                root_left.min(root_right),
            );
        }
    }
    parent
}

fn cluster(
    files: &BTreeMap<String, NodeIndex>,
    hubs: &BTreeSet<String>,
    parent: &BTreeMap<String, String>,
) -> Vec<Cluster> {
    let mut groups = BTreeMap::<String, Vec<(String, NodeIndex)>>::new();
    for (path, index) in files {
        let key = if hubs.contains(path) {
            path.clone()
        } else {
            root(parent, &directory(path))
        };
        groups.entry(key).or_default().push((path.clone(), *index));
    }
    let mut clusters = groups
        .into_iter()
        .filter_map(|(key, mut members)| {
            members.sort_by(|left, right| left.0.cmp(&right.0));
            let utility = members.iter().any(|(path, _)| hubs.contains(path));
            if !utility && members.len() < 2 {
                return None;
            }
            Some(Cluster {
                key,
                kind: if utility { "utility" } else { "subsystem" },
                files: members.iter().map(|(_, index)| *index).collect(),
                paths: members.into_iter().map(|(path, _)| path).collect(),
            })
        })
        .collect::<Vec<_>>();
    clusters.sort_by(|left, right| {
        right
            .files
            .len()
            .cmp(&left.files.len())
            .then_with(|| left.key.cmp(&right.key))
    });
    clusters
}

fn file_keys(clusters: &[Cluster]) -> BTreeMap<String, String> {
    clusters
        .iter()
        .flat_map(|cluster| {
            cluster
                .paths
                .iter()
                .map(|path| (path.clone(), cluster.key.clone()))
        })
        .collect()
}

fn boundaries(
    pairs: &[Pair],
    hubs: &BTreeSet<String>,
    keys: &BTreeMap<String, String>,
) -> Vec<Boundary> {
    pairs
        .iter()
        .filter_map(|pair| {
            let from_key = keys.get(&pair.left)?.clone();
            let to_key = keys.get(&pair.right)?.clone();
            if from_key == to_key {
                return None;
            }
            Some(Boundary {
                from: pair.left.clone(),
                to: pair.right.clone(),
                from_key,
                to_key,
                relation: pair.relations.iter().next()?.clone(),
                why: if hubs.contains(&pair.left) || hubs.contains(&pair.right) {
                    "utility_hub"
                } else {
                    "weak_bridge"
                },
            })
        })
        .collect()
}

fn directory(path: &str) -> String {
    match path.rfind('/') {
        Some(index) => path[..index].to_owned(),
        None => "(root)".to_owned(),
    }
}

fn root(parent: &BTreeMap<String, String>, node: &str) -> String {
    let mut current = node.to_owned();
    for _ in 0..64 {
        match parent.get(&current) {
            Some(next) if next != &current => current = next.clone(),
            _ => break,
        }
    }
    current
}

fn ordered(left: &str, right: &str) -> (String, String) {
    if left <= right {
        (left.to_owned(), right.to_owned())
    } else {
        (right.to_owned(), left.to_owned())
    }
}
