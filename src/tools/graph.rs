use crate::RepositoryState;
use crate::tools::graph_walk::{resolve_seeds, traverse};
use crate::tools::{arg_bool, arg_str, arg_u64};
use serde_json::{Value, json};
use std::collections::BTreeMap;
use weavatrix_graph::{Direction, NodeIndex, NodeKind, shortest_path, weakly_connected_components};

pub fn stats(state: &RepositoryState) -> Value {
    let mut kinds = BTreeMap::<String, u64>::new();
    let mut relations = BTreeMap::<String, u64>::new();
    let mut evidence = BTreeMap::<String, u64>::new();
    for node in state.graph().nodes() {
        *kinds.entry(node.kind.as_str().to_owned()).or_default() += 1;
    }
    for edge in state.graph().edges() {
        *relations.entry(edge.kind.as_str().to_owned()).or_default() += 1;
        *evidence
            .entry(edge.provenance.evidence.as_str().to_owned())
            .or_default() += 1;
    }
    json!({
        "repository": state.snapshot().repository,
        "revision": state.snapshot().revision,
        "nodes": state.graph().node_count(),
        "edges": state.graph().edge_count(),
        "build_ms": state.build_time().as_secs_f64() * 1000.0,
        "node_kinds": kinds,
        "relations": relations,
        "evidence": evidence,
        "capabilities": state.snapshot().capabilities,
        "freshness": {
            "state": "CURRENT",
            "source_revision": state.snapshot().revision,
            "incremental_hashes_reused": state.scan_report().cache.reused_hashes,
            "content_reads": state.scan_report().cache.content_reads
        }
    })
}

pub fn get_node(state: &RepositoryState, args: &Value) -> Result<Value, String> {
    let index = state.resolve_node(arg_str(args, "label")?)?;
    let node = state.node(index)?;
    Ok(json!({
        "node": node,
        "incoming": state.graph().in_degree(index),
        "outgoing": state.graph().out_degree(index)
    }))
}

pub fn neighbors(state: &RepositoryState, args: &Value) -> Result<Value, String> {
    let index = state.resolve_node(arg_str(args, "label")?)?;
    let filter = arg_str(args, "relation_filter").ok();
    let mut items = Vec::new();
    for (direction, edges) in [
        (
            "outgoing",
            state.graph().outgoing_at(index).collect::<Vec<_>>(),
        ),
        (
            "incoming",
            state.graph().incoming_at(index).collect::<Vec<_>>(),
        ),
    ] {
        for edge in edges {
            if filter.is_some_and(|value| edge.kind.as_str() != value) {
                continue;
            }
            let other = if direction == "outgoing" {
                state.graph().node(edge.target.as_str())
            } else {
                state.graph().node(edge.source.as_str())
            };
            items.push(json!({"direction": direction, "edge": edge, "node": other}));
        }
    }
    Ok(json!({"node": state.node(index)?, "neighbors": items}))
}

pub fn query(state: &RepositoryState, args: &Value) -> Result<Value, String> {
    let seeds = resolve_seeds(state, args)?;
    let depth = usize::try_from(arg_u64(args, "depth").unwrap_or(3)).unwrap_or(3);
    let max_nodes = usize::try_from(arg_u64(args, "max_nodes").unwrap_or(80)).unwrap_or(80);
    let direction = match arg_str(args, "flow_direction").unwrap_or("both") {
        "forward" => Direction::Outgoing,
        "backward" => Direction::Incoming,
        _ => Direction::Both,
    };
    let dfs = arg_str(args, "mode").unwrap_or("bfs") == "dfs";
    let relations = relation_filter(args);
    let (visited, traversed) = traverse(
        state,
        seeds,
        depth,
        max_nodes,
        direction,
        dfs,
        relations.as_ref(),
    );
    let nodes = visited
        .iter()
        .filter_map(|index| state.graph().node_at(*index))
        .collect::<Vec<_>>();
    let edges = traversed
        .into_iter()
        .filter_map(|index| state.graph().edge_at(index))
        .collect::<Vec<_>>();
    Ok(json!({"nodes": nodes, "edges": edges, "truncated": nodes.len() == max_nodes}))
}

pub fn hubs(state: &RepositoryState, args: &Value) -> Value {
    let top = usize::try_from(arg_u64(args, "top_n").unwrap_or(10)).unwrap_or(10);
    let mut nodes = state
        .graph()
        .nodes()
        .iter()
        .enumerate()
        .filter(|(_, node)| !matches!(node.kind, NodeKind::Repository | NodeKind::Package))
        .map(|(slot, node)| {
            let index = NodeIndex::new(u32::try_from(slot).unwrap_or(u32::MAX));
            let incoming = state.graph().in_degree(index).unwrap_or(0);
            let outgoing = state.graph().out_degree(index).unwrap_or(0);
            (incoming + outgoing, incoming, outgoing, node)
        })
        .collect::<Vec<_>>();
    nodes.sort_unstable_by(|left, right| {
        right
            .0
            .cmp(&left.0)
            .then_with(|| left.3.id.cmp(&right.3.id))
    });
    json!({
        "hubs": nodes.into_iter().take(top).map(|(degree, incoming, outgoing, node)| {
            json!({"node": node, "degree": degree, "incoming": incoming, "outgoing": outgoing})
        }).collect::<Vec<_>>()
    })
}

pub fn path(state: &RepositoryState, args: &Value) -> Result<Value, String> {
    let source = state.resolve_node(arg_str(args, "source")?)?;
    let target = state.resolve_node(arg_str(args, "target")?)?;
    let mut indices = shortest_path(state.graph(), source, target).unwrap_or_default();
    let max_hops = usize::try_from(arg_u64(args, "max_hops").unwrap_or(8)).unwrap_or(8);
    let bounded_out = indices.len().saturating_sub(1) > max_hops;
    if bounded_out {
        indices.clear();
    }
    let nodes = indices
        .iter()
        .filter_map(|index| state.graph().node_at(*index))
        .collect::<Vec<_>>();
    Ok(json!({
        "found": !nodes.is_empty(),
        "bounded_out": bounded_out,
        "max_hops": max_hops,
        "hops": nodes.len().saturating_sub(1),
        "nodes": nodes
    }))
}

pub fn dependents(state: &RepositoryState, args: &Value) -> Result<Value, String> {
    let seed = state.resolve_node(arg_str(args, "label")?)?;
    let depth = usize::try_from(arg_u64(args, "depth").unwrap_or(3)).unwrap_or(3);
    let max = usize::try_from(arg_u64(args, "max_nodes").unwrap_or(40)).unwrap_or(40);
    let (visited, _) = traverse(
        state,
        vec![seed],
        depth,
        max + 1,
        Direction::Incoming,
        false,
        None,
    );
    let nodes = visited
        .into_iter()
        .skip(1)
        .filter_map(|index| state.graph().node_at(index))
        .collect::<Vec<_>>();
    Ok(json!({
        "seed": state.node(seed)?,
        "dependents": nodes,
        "precision": "graph",
        "semantic_precision": "BOUNDED_STATIC"
    }))
}

pub fn communities(state: &RepositoryState, args: &Value) -> Result<Value, String> {
    let mut components = weakly_connected_components(state.graph());
    components.sort_unstable_by_key(|right| std::cmp::Reverse(right.len()));
    if let Ok(id) = arg_u64(args, "community_id") {
        let id = usize::try_from(id).map_err(|_| "community_id is too large")?;
        let component = components
            .get(id)
            .ok_or_else(|| format!("community not found: {id}"))?;
        return Ok(json!({"community_id": id, "nodes": component.iter()
            .filter_map(|index| state.graph().node_at(*index)).collect::<Vec<_>>() }));
    }
    let top = usize::try_from(arg_u64(args, "top_n").unwrap_or(20)).unwrap_or(20);
    Ok(
        json!({"communities": components.iter().take(top).enumerate().map(|(id, nodes)| {
        json!({"community_id": id, "nodes": nodes.len(), "sample": nodes.iter().take(5)
            .filter_map(|index| state.graph().node_at(*index).map(|node| &node.label)).collect::<Vec<_>>()})
    }).collect::<Vec<_>>()}),
    )
}

pub fn module_map(state: &RepositoryState, args: &Value) -> Value {
    let top = usize::try_from(arg_u64(args, "top_n").unwrap_or(25)).unwrap_or(25);
    let mut modules = BTreeMap::<String, (u64, u64)>::new();
    for node in state.graph().nodes() {
        let Some(path) = node
            .span
            .as_ref()
            .map(|span| &span.file)
            .or_else(|| (node.kind == NodeKind::File).then_some(&node.label))
        else {
            continue;
        };
        let module = path.split('/').next().unwrap_or("(root)").to_owned();
        let entry = modules.entry(module).or_default();
        if node.kind == NodeKind::File {
            entry.0 += 1;
        } else {
            entry.1 += 1;
        }
    }
    let mut modules = modules.into_iter().collect::<Vec<_>>();
    modules.sort_unstable_by(|left, right| {
        (right.1.0 + right.1.1)
            .cmp(&(left.1.0 + left.1.1))
            .then_with(|| left.0.cmp(&right.0))
    });
    json!({"modules": modules.into_iter().take(top).map(|(path, (files, symbols))| {
        json!({"path": path, "files": files, "symbols": symbols})
    }).collect::<Vec<_>>()})
}

pub fn endpoints(state: &RepositoryState, args: &Value) -> Result<Value, String> {
    let method = arg_str(args, "method").ok();
    let path = arg_str(args, "path").ok();
    let mut endpoints = state
        .graph()
        .nodes()
        .iter()
        .filter(|node| node.kind == NodeKind::Endpoint)
        .filter(|node| method.is_none_or(|value| node.label.starts_with(value)))
        .filter(|node| path.is_none_or(|value| node.label.ends_with(value)))
        .collect::<Vec<_>>();
    endpoints.sort_unstable_by(|left, right| left.label.cmp(&right.label));
    if let Some(path) = path
        && arg_bool(args, "trace").unwrap_or(false)
    {
        let endpoint = endpoints
            .first()
            .ok_or_else(|| format!("endpoint not found: {path}"))?;
        return neighbors(state, &json!({"label": endpoint.id.as_str()}));
    }
    Ok(json!({"endpoints": endpoints}))
}

fn relation_filter(args: &Value) -> Option<std::collections::BTreeSet<String>> {
    let value = args.get("relation_filter")?;
    if let Some(value) = value.as_str() {
        return Some(std::collections::BTreeSet::from([value.to_owned()]));
    }
    Some(
        value
            .as_array()?
            .iter()
            .filter_map(Value::as_str)
            .map(str::to_owned)
            .collect(),
    )
}
