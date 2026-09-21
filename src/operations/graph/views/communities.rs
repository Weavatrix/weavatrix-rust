use super::super::pagination::page_offset;
use super::projection::{Cluster, Projection, project};
use crate::engine::RepositoryState;
use crate::operations::architecture::contract;
use crate::operations::{arg_str, arg_u64, optional_bool, optional_u64};
use blazingly_json::{Value, json};
use weavatrix_graph::{NodeIndex, NodeKind};

pub fn communities(state: &RepositoryState, args: &Value) -> Result<Value, String> {
    match arg_str(args, "view").unwrap_or("subsystems") {
        "connectivity" => connectivity(state, args),
        "subsystems" => subsystems(state, args),
        _ => Err("view must be subsystems or connectivity".to_owned()),
    }
}

fn connectivity(state: &RepositoryState, args: &Value) -> Result<Value, String> {
    let components = state.coupled_components();
    if let Ok(id) = arg_u64(args, "community_id") {
        return one_component(state, components, id, args);
    }
    let top = usize::try_from(arg_u64(args, "top_n").unwrap_or(20)).unwrap_or(20);
    Ok(json!({
        "view": "connectivity",
        "algorithm": "weak-coupling-components",
        "interpretation": "connectivity",
        "communities": components.iter().take(top).enumerate().map(|(id, nodes)| {
            json!({
                "community_id": id,
                "nodes": nodes.len(),
                "sample": nodes.iter().take(5)
                    .filter_map(|index| state.graph().node_at(*index).map(|node| &node.label))
                    .collect::<Vec<_>>()
            })
        }).collect::<Vec<_>>()
    }))
}

fn one_component(
    state: &RepositoryState,
    components: &[Vec<NodeIndex>],
    id: u64,
    args: &Value,
) -> Result<Value, String> {
    let id = usize::try_from(id).map_err(|_| "community_id is too large")?;
    let component = components
        .get(id)
        .ok_or_else(|| format!("community not found: {id}"))?;
    let (nodes, page) = page_nodes(state, component, args)?;
    Ok(json!({
        "view": "connectivity",
        "community_id": id,
        "interpretation": "connectivity",
        "nodes": nodes,
        "page": page
    }))
}

fn subsystems(state: &RepositoryState, args: &Value) -> Result<Value, String> {
    let declaration = contract::load_optional(state)?;
    let include_non_product = optional_bool(args, "include_non_product")?.unwrap_or(false);
    let resolution = optional_u64(args, "resolution")?.unwrap_or(2);
    let hub_degree = optional_u64(args, "hub_degree")?.unwrap_or(3);
    if resolution == 0 || hub_degree == 0 {
        return Err("resolution and hub_degree must be at least 1".to_owned());
    }
    let projection = project(state, include_non_product, resolution, hub_degree);
    if let Ok(id) = arg_u64(args, "community_id") {
        return one_cluster(state, &projection, id, args, declaration.as_ref());
    }
    let top = usize::try_from(arg_u64(args, "top_n").unwrap_or(20)).unwrap_or(20);
    Ok(json!({
        "view": "subsystems",
        "algorithm": "directory-resolution",
        "interpretation": "derived_projection",
        "parameters": {
            "resolution": projection.resolution,
            "hub_degree": projection.hub_degree,
            "relations": super::super::coupling::coupling_relations().iter().collect::<Vec<_>>()
        },
        "communities": projection.clusters.iter().take(top).enumerate().map(|(id, cluster)| {
            summary(id, cluster, declaration.as_ref())
        }).collect::<Vec<_>>(),
        "boundaries": projection.boundaries.iter().take(40).map(|item| {
            json!({
                "from": item.from,
                "to": item.to,
                "from_community": item.from_key,
                "to_community": item.to_key,
                "relation": item.relation,
                "why": item.why
            })
        }).collect::<Vec<_>>()
    }))
}

fn one_cluster(
    state: &RepositoryState,
    projection: &Projection,
    id: u64,
    args: &Value,
    declaration: Option<&Value>,
) -> Result<Value, String> {
    let id = usize::try_from(id).map_err(|_| "community_id is too large")?;
    let cluster = projection
        .clusters
        .get(id)
        .ok_or_else(|| format!("community not found: {id}"))?;
    let members = members(state, cluster);
    let (nodes, page) = page_nodes(state, &members, args)?;
    let touching = projection
        .boundaries
        .iter()
        .filter(|item| item.from_key == cluster.key || item.to_key == cluster.key)
        .map(|item| {
            json!({
                "from": item.from,
                "to": item.to,
                "from_community": item.from_key,
                "to_community": item.to_key,
                "relation": item.relation,
                "why": item.why
            })
        })
        .collect::<Vec<_>>();
    Ok(json!({
        "view": "subsystems",
        "community_id": id,
        "community_key": cluster.key,
        "kind": cluster.kind,
        "interpretation": "derived_projection",
        "declared_components": declared(declaration, cluster),
        "nodes": nodes,
        "boundaries": touching,
        "page": page
    }))
}

fn summary(id: usize, cluster: &Cluster, declaration: Option<&Value>) -> Value {
    json!({
        "community_id": id,
        "community_key": cluster.key,
        "kind": cluster.kind,
        "nodes": cluster.files.len(),
        "sample": cluster.paths.iter().take(5).collect::<Vec<_>>(),
        "declared_components": declared(declaration, cluster)
    })
}

fn declared(declaration: Option<&Value>, cluster: &Cluster) -> Vec<String> {
    let mut ids = cluster
        .paths
        .iter()
        .flat_map(|path| {
            declaration
                .into_iter()
                .flat_map(|value| contract::components_for(value, path))
        })
        .map(str::to_owned)
        .collect::<Vec<_>>();
    ids.sort();
    ids.dedup();
    ids
}

fn members(state: &RepositoryState, cluster: &Cluster) -> Vec<NodeIndex> {
    let files = cluster
        .paths
        .iter()
        .cloned()
        .collect::<std::collections::BTreeSet<_>>();
    let mut nodes = cluster.files.clone();
    for (slot, node) in state.graph().nodes().iter().enumerate() {
        if node.kind == NodeKind::File {
            continue;
        }
        let Some(path) = node.span.as_ref().map(|span| span.file.as_str()) else {
            continue;
        };
        if files.contains(path) {
            nodes.push(NodeIndex::new(u32::try_from(slot).unwrap_or(u32::MAX)));
        }
    }
    nodes.sort_by_key(|index| index.index());
    nodes.dedup();
    nodes
}

fn page_nodes<'graph>(
    state: &'graph RepositoryState,
    members: &[NodeIndex],
    args: &Value,
) -> Result<(Vec<&'graph weavatrix_graph::Node>, Value), String> {
    let offset = page_offset(args)?;
    if offset > members.len() {
        return Err("cursor offset is outside the selected community".to_owned());
    }
    let max_nodes = usize::try_from(arg_u64(args, "max_nodes").unwrap_or(50)).unwrap_or(50);
    if max_nodes == 0 || max_nodes > 500 {
        return Err("max_nodes must be between 1 and 500".to_owned());
    }
    let end = offset.saturating_add(max_nodes).min(members.len());
    let nodes = members[offset..end]
        .iter()
        .filter_map(|index| state.graph().node_at(*index))
        .collect::<Vec<_>>();
    let returned = nodes.len();
    Ok((
        nodes,
        json!({
            "offset": offset,
            "returned": returned,
            "total": members.len(),
            "has_more": end < members.len(),
            "next_cursor": (end < members.len()).then(|| format!("v1:{end}"))
        }),
    ))
}
