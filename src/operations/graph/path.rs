use super::relation_filter;
use crate::engine::RepositoryState;
use crate::operations::{arg_str, arg_u64, optional_bool, optional_str};
use blazingly_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};
use weavatrix_graph::{Direction, EdgeIndex, NodeIndex};

pub fn path(state: &RepositoryState, args: &Value) -> Result<Value, String> {
    let source = state.resolve_node(arg_str(args, "source")?)?;
    let target = state.resolve_node(arg_str(args, "target")?)?;
    let max_hops = usize::try_from(arg_u64(args, "max_hops").unwrap_or(8)).unwrap_or(8);
    let max_edges = usize::try_from(arg_u64(args, "max_edges").unwrap_or(4096)).unwrap_or(4096);
    if max_edges == 0 {
        return Err("max_edges must be at least 1".to_owned());
    }
    let relations = path_relations(args)?;
    let direction = path_direction(args)?;
    let search = bounded_path(
        state,
        source,
        target,
        direction,
        max_hops,
        max_edges,
        relations.as_ref(),
    );
    let hops = hop_witnesses(state, &search.nodes, direction, relations.as_ref());
    let (path_kind, execution) = classify(&hops);
    let incomplete = search.bounded_hops || search.bounded_work;
    Ok(json!({
        "found": !search.nodes.is_empty(),
        "bounded_out": incomplete,
        "max_hops": max_hops,
        "max_edges": max_edges,
        "hops": search.nodes.len().saturating_sub(1),
        "nodes": search.nodes.iter().filter_map(|index| state.graph().node_at(*index)).collect::<Vec<_>>(),
        "witnesses": hops,
        "path_kind": path_kind,
        "is_execution_chain": execution,
        "execution_status": "OK",
        "evidence_completeness": if incomplete { "INCOMPLETE" } else { "COMPLETE" },
        "stop_reason": stop_reason(!search.nodes.is_empty(), search.bounded_hops, search.bounded_work)
    }))
}

fn path_relations(args: &Value) -> Result<Option<BTreeSet<String>>, String> {
    let explicit = relation_filter(args)?;
    let Some(kind) = optional_str(args, "path_kind")? else {
        return Ok(explicit);
    };
    let preset = kind_relations(kind)?;
    Ok(Some(match explicit {
        Some(filter) => filter.intersection(&preset).cloned().collect(),
        None => preset,
    }))
}

fn kind_relations(kind: &str) -> Result<BTreeSet<String>, String> {
    let names = match kind {
        "calls" => &["calls", "calls_workflow"][..],
        "dependency" => &[
            "imports",
            "inherits",
            "implements",
            "re_exports",
            "references",
            "depends_on_output",
        ],
        "documentation" => &["configures"],
        _ => {
            return Err("path_kind must be calls, dependency, or documentation".to_owned());
        }
    };
    Ok(names.iter().map(|name| (*name).to_owned()).collect())
}

fn path_direction(args: &Value) -> Result<Direction, String> {
    if let Ok(value) = arg_str(args, "flow_direction") {
        return Ok(match value {
            "backward" => Direction::Incoming,
            "both" => Direction::Both,
            "forward" => Direction::Outgoing,
            _ => return Err("flow_direction must be forward, backward, or both".to_owned()),
        });
    }
    Ok(match optional_bool(args, "directed")? {
        Some(false) => Direction::Both,
        _ => Direction::Outgoing,
    })
}

struct Search {
    nodes: Vec<NodeIndex>,
    bounded_hops: bool,
    bounded_work: bool,
}

fn bounded_path(
    state: &RepositoryState,
    source: NodeIndex,
    target: NodeIndex,
    direction: Direction,
    max_hops: usize,
    max_edges: usize,
    relations: Option<&BTreeSet<String>>,
) -> Search {
    let mut seen = BTreeMap::new();
    let mut queue = std::collections::VecDeque::new();
    let mut bounded_hops = false;
    let mut work = 0_usize;
    seen.insert(source, None);
    queue.push_back((source, 0_usize));
    while let Some((node, depth)) = queue.pop_front() {
        if node == target {
            return Search {
                nodes: rebuild(source, target, &seen),
                bounded_hops: false,
                bounded_work: false,
            };
        }
        if depth >= max_hops {
            bounded_hops = true;
            continue;
        }
        if !expand(
            state, node, depth, direction, relations, max_edges, &mut seen, &mut queue, &mut work,
        ) {
            return Search {
                nodes: Vec::new(),
                bounded_hops,
                bounded_work: true,
            };
        }
    }
    Search {
        nodes: Vec::new(),
        bounded_hops,
        bounded_work: work >= max_edges,
    }
}

#[allow(clippy::too_many_arguments)]
fn expand(
    state: &RepositoryState,
    node: NodeIndex,
    depth: usize,
    direction: Direction,
    relations: Option<&BTreeSet<String>>,
    max_edges: usize,
    seen: &mut BTreeMap<NodeIndex, Option<(NodeIndex, EdgeIndex)>>,
    queue: &mut std::collections::VecDeque<(NodeIndex, usize)>,
    work: &mut usize,
) -> bool {
    for edge in super::walk::adjacent(state, node, direction) {
        if *work >= max_edges {
            return false;
        }
        *work += 1;
        if !super::walk::keep(state, edge, relations) {
            continue;
        }
        let Some(next) = super::walk::neighbor(state, node, edge) else {
            continue;
        };
        if seen.contains_key(&next) {
            continue;
        }
        seen.insert(next, Some((node, edge)));
        queue.push_back((next, depth + 1));
    }
    true
}

fn rebuild(
    source: NodeIndex,
    target: NodeIndex,
    seen: &BTreeMap<NodeIndex, Option<(NodeIndex, EdgeIndex)>>,
) -> Vec<NodeIndex> {
    let mut path = vec![target];
    let mut current = target;
    while current != source {
        let Some(Some((prev, _))) = seen.get(&current).copied() else {
            return Vec::new();
        };
        path.push(prev);
        current = prev;
    }
    path.reverse();
    path
}

fn hop_witnesses(
    state: &RepositoryState,
    nodes: &[NodeIndex],
    direction: Direction,
    relations: Option<&BTreeSet<String>>,
) -> Vec<Value> {
    nodes
        .windows(2)
        .filter_map(|pair| hop_json(state, pair[0], pair[1], direction, relations))
        .collect()
}

fn hop_json(
    state: &RepositoryState,
    from: NodeIndex,
    to: NodeIndex,
    direction: Direction,
    relations: Option<&BTreeSet<String>>,
) -> Option<Value> {
    let origin = state.graph().node_at(from)?;
    let dest = state.graph().node_at(to)?;
    let directed = super::walk::adjacent(state, from, direction).collect::<BTreeSet<_>>();
    let mut relations_json = Vec::new();
    let mut traversed = None;
    for edge in super::walk::adjacent(state, from, Direction::Both) {
        if super::walk::neighbor(state, from, edge) != Some(to) {
            continue;
        }
        let data = state.graph().edge_at(edge)?;
        let item = json!({
            "relation": data.kind,
            "provenance": data.provenance
        });
        if traversed.is_none()
            && directed.contains(&edge)
            && super::walk::keep(state, edge, relations)
        {
            traversed = Some(item.clone());
        }
        relations_json.push(item);
    }
    let first = traversed.or_else(|| relations_json.first().cloned())?;
    Some(json!({
        "from": origin.id,
        "to": dest.id,
        "relation": first["relation"],
        "provenance": first["provenance"],
        "relations": relations_json
    }))
}

fn classify(hops: &[Value]) -> (&'static str, bool) {
    let mut calls = false;
    let mut dependency = false;
    let mut documentation = false;
    for hop in hops {
        for item in hop["relations"].as_array().into_iter().flatten() {
            match item["relation"].as_str().unwrap_or("") {
                "calls" | "calls_workflow" => calls = true,
                "configures" => documentation = true,
                _ => dependency = true,
            }
        }
    }
    match (calls, dependency, documentation) {
        (false, false, false) => ("none", false),
        (true, false, false) => ("calls", true),
        (false, true, false) => ("dependency", false),
        (false, false, true) => ("documentation", false),
        _ => ("mixed", false),
    }
}

fn stop_reason(found: bool, hops: bool, work: bool) -> &'static str {
    if found {
        "COMPLETE"
    } else if hops {
        "MAX_HOPS"
    } else if work {
        "MAX_EDGES"
    } else {
        "COMPLETE"
    }
}
