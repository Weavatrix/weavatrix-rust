use crate::operations::graph;
use crate::operations::node_path;
use blazingly_json::{Value, json};
use std::collections::{BTreeSet, VecDeque};
use weavatrix_graph::{Graph, Node, NodeKind};

pub(super) fn reverse_hits(
    graph: &Graph,
    seed: &Node,
    depth: usize,
    max: usize,
    revision: &str,
    change: &str,
    coarse: bool,
) -> (Vec<Value>, bool) {
    let allowed = graph::coupling_relations();
    let mut visited = BTreeSet::from([seed.id.as_str().to_owned()]);
    let mut queue = VecDeque::from([(seed.id.as_str().to_owned(), 0_usize)]);
    let mut hits = Vec::new();
    let mut capped = false;
    while let Some((id, distance)) = queue.pop_front() {
        if distance >= depth {
            continue;
        }
        let Some(index) = graph.node_index(&id) else {
            continue;
        };
        for edge in graph.incoming_at(index) {
            if !allowed.contains(edge.kind.as_str()) {
                continue;
            }
            let other = edge.source.as_str();
            if !visited.insert(other.to_owned()) {
                continue;
            }
            let Some(node) = graph.node(other) else {
                continue;
            };
            if matches!(node.kind, NodeKind::Repository | NodeKind::Package) {
                continue;
            }
            if hits.len() >= max {
                capped = true;
                break;
            }
            push_hit(
                &mut hits,
                &Hit {
                    graph,
                    node,
                    seed,
                    distance: distance + 1,
                    relation: edge.kind.as_str(),
                    revision,
                    change,
                    coarse,
                },
            );
            queue.push_back((other.to_owned(), distance + 1));
        }
        if capped {
            break;
        }
    }
    (hits, capped)
}

struct Hit<'a> {
    graph: &'a Graph,
    node: &'a Node,
    seed: &'a Node,
    distance: usize,
    relation: &'a str,
    revision: &'a str,
    change: &'a str,
    coarse: bool,
}

fn push_hit(hits: &mut Vec<Value>, hit: &Hit<'_>) {
    hits.push(json!({
        "seed": hit.seed.id,
        "change": hit.change,
        "relation": hit.relation,
        "distance": hit.distance,
        "revision": hit.revision,
        "coarse": hit.coarse,
        "node": hit.node
    }));
    if hit.node.kind != NodeKind::File
        && let Some(path) = node_path(hit.node)
        && let Some(file) = file_node(hit.graph, path)
    {
        hits.push(json!({
            "seed": hit.seed.id,
            "change": hit.change,
            "relation": "contains",
            "distance": hit.distance,
            "revision": hit.revision,
            "coarse": hit.coarse,
            "node": file
        }));
    }
}

pub(super) fn file_node<'a>(graph: &'a Graph, path: &str) -> Option<&'a Node> {
    graph
        .nodes()
        .iter()
        .find(|node| node.kind == NodeKind::File && node.label == path)
}
