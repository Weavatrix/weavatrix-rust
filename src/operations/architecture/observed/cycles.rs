use super::components::Component;
use blazingly_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};
use weavatrix_graph::{
    EdgeEndpoints, NodeIndex, Topology, condensation_filtered, find_cycle_filtered,
    strongly_connected_components_filtered, topological_generations_filtered,
};

pub(super) fn analyze(components: &[Component], edges: &[Value]) -> Result<Value, String> {
    let index = components
        .iter()
        .enumerate()
        .map(|(slot, component)| (component.id.as_str(), slot))
        .collect::<BTreeMap<_, _>>();
    let links = edges
        .iter()
        .filter_map(|edge| {
            let (from, to) = (edge["from"].as_str()?, edge["to"].as_str()?);
            Some(EdgeEndpoints::new(
                compact(*index.get(from)?).ok()?,
                compact(*index.get(to)?).ok()?,
            ))
        })
        .collect::<Vec<_>>();
    let topology =
        Topology::try_from_edges(components.len(), links).map_err(|error| error.to_string())?;
    let groups = strongly_connected_components_filtered(&topology, |_| true);
    let mut cyclic = groups
        .into_iter()
        .filter(|group| group.len() > 1)
        .map(|group| cycle_record(&topology, components, edges, &group))
        .collect::<Vec<_>>();
    cyclic.sort_by(|left, right| {
        left["members"]
            .to_string()
            .cmp(&right["members"].to_string())
    });
    let cyclic_total = cyclic.len();
    let condensed =
        condensation_filtered(&topology, |_| true).map_err(|error| error.to_string())?;
    let levels = topological_generations_filtered(condensed.topology(), |_| true)
        .unwrap_or_default()
        .len();
    Ok(json!({
        "semantics": "component_quotient_dependency; not symbol recursion or a single executable configuration",
        "relation_scope": "all observed coupling relations; context applicability unknown",
        "scc_total": condensed.components().len(),
        "cyclic_scc_total": cyclic_total,
        "cyclic_scc": cyclic.into_iter().take(50).collect::<Vec<_>>(),
        "cyclic_scc_truncated": cyclic_total > 50,
        "classification": "QUOTIENT_UNION_CYCLE_CANDIDATE",
        "condensation_nodes": condensed.topology().node_count(),
        "condensation_edges": condensed.topology().edge_count(),
        "dag_levels": levels
    }))
}

fn cycle_record(
    topology: &Topology,
    components: &[Component],
    edges: &[Value],
    group: &[NodeIndex],
) -> Value {
    let allowed = group.iter().copied().collect::<BTreeSet<_>>();
    let path = find_cycle_filtered(topology, |edge| {
        topology.edge_endpoints(edge).is_some_and(|ends| {
            allowed.contains(&ends.source()) && allowed.contains(&ends.target())
        })
    })
    .unwrap_or_default();
    let witness = path
        .windows(2)
        .filter_map(|pair| {
            let from = &components[pair[0].index()].id;
            let to = &components[pair[1].index()].id;
            let aggregate = edges.iter().find(|edge| {
                edge["from"].as_str() == Some(from) && edge["to"].as_str() == Some(to)
            })?;
            Some(json!({
                "from": from,
                "to": to,
                "relation": aggregate["relation"],
                "base_edge_indices": aggregate["base_edge_indices"],
                "evidence_sample": aggregate["evidence_sample"]
            }))
        })
        .collect::<Vec<_>>();
    let mut members = group
        .iter()
        .map(|node| components[node.index()].id.clone())
        .collect::<Vec<_>>();
    members.sort();
    json!({
        "members": members,
        "component_edge_witness": witness,
        "lifting_status": "LOWER_GRAPH_CYCLE_NOT_PROVEN",
        "configuration_status": "CONDITION_COMPATIBILITY_NOT_PROVEN"
    })
}

fn compact(index: usize) -> Result<NodeIndex, std::num::TryFromIntError> {
    Ok(NodeIndex::new(u32::try_from(index)?))
}
