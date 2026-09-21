//! Observed folders, packages, and typed edges. This is not a style label
//! and not the starter contract.

mod components;
mod edges;
mod packages;

use crate::engine::RepositoryState;
use crate::model::digest::sha3_256;
use crate::operations::{optional_str, optional_u64, reject_unknown_arguments};
use blazingly_json::{Value, json};
use weavatrix_graph::{
    EdgeEndpoints, NodeIndex, Topology, condensation_filtered,
    strongly_connected_components_filtered, topological_generations_filtered,
};

pub(super) fn attach(state: &RepositoryState, mut report: Value) -> Value {
    if let Some(object) = report.as_object_mut() {
        object.insert(
            "observed".to_owned(),
            facts(state, None).expect("unpaginated projection"),
        );
    }
    report
}

pub(super) fn report(state: &RepositoryState, args: &Value) -> Result<Value, String> {
    reject_unknown_arguments(
        "architecture_inventory",
        args,
        &["max_results", "edge_cursor", "token_budget"],
    )?;
    let max = optional_u64(args, "max_results")?.unwrap_or(200).min(500) as usize;
    if max == 0 {
        return Err("max_results must be at least 1".to_owned());
    }
    let cursor = optional_str(args, "edge_cursor")?;
    facts(state, Some((max, cursor)))
}

pub(crate) fn memberships(state: &RepositoryState) -> Vec<(String, String)> {
    components::collect(state, None)
        .into_iter()
        .map(|component| (component.path, component.id))
        .collect()
}

fn facts(state: &RepositoryState, page: Option<(usize, Option<&str>)>) -> Result<Value, String> {
    let (declaration, diagnostic) = match super::contract::load_optional(state) {
        Ok(value) => (value, None),
        Err(error) => (None, Some(error)),
    };
    let components = components::collect(state, declaration.as_ref());
    let edges = edges::collect(state, &components);
    let cycles = cycles(&components, &edges)?;
    let total = edges.len();
    // A cursor belongs to these captured graph facts, not just a revision name.
    let identity = sha3_256(
        &blazingly_json::to_vec(&json!({
            "components": components.iter().map(component_json).collect::<Vec<_>>(),
            "edges": edges
        }))
        .map_err(|error| error.to_string())?,
    );
    let (max, start) = if let Some((max, cursor)) = page {
        let start = if let Some(cursor) = cursor {
            let (hash, offset) = cursor.rsplit_once(':').ok_or("invalid edge_cursor")?;
            if hash != identity {
                return Err("edge_cursor belongs to another analysis".to_owned());
            }
            let offset = offset.parse::<usize>().map_err(|_| "invalid edge_cursor")?;
            if offset > total {
                return Err("edge_cursor is out of range".to_owned());
            }
            offset
        } else {
            0
        };
        (max, start)
    } else {
        (200, 0)
    };
    let returned = edges
        .iter()
        .skip(start)
        .take(max)
        .cloned()
        .collect::<Vec<_>>();
    let next = start.saturating_add(returned.len());
    let truncated = next < total;
    Ok(json!({
        "kind": "observed",
        "model": "structural directory fallback and typed graph edges; not a style label or build-module claim",
        "view": "structural_directory_fallback.v1",
        "status": if diagnostic.is_some() || truncated || cycles["cyclic_scc_truncated"] == true {"INCOMPLETE"} else {"COMPLETE"},
        "analysis": {"coverage": "BOUNDED_OBSERVATION", "closed_world": false,
                     "discovered_relations_evaluated": true,
                     "unknowns_total": usize::from(diagnostic.is_some())},
        "diagnostics": diagnostic.into_iter().collect::<Vec<_>>(),
        "packages": packages::collect(state),
        "components": components.iter().map(component_json).collect::<Vec<_>>(),
        "edges": returned,
        "edges_total": total,
        "edges_returned": next - start,
        "edges_truncated": truncated,
        "next_edge_cursor": truncated.then(|| format!("{identity}:{next}")),
        "cycles": cycles
    }))
}

fn component_json(component: &components::Component) -> Value {
    let mut value = json!({
        "id": component.id,
        "path": component.path,
        "files": component.files.len(),
        "declared_ids": component.declared_ids,
        "view": "structural_directory_fallback.v1"
    });
    if let (Some(declared), Some(object)) = (&component.declared_id, value.as_object_mut()) {
        object.insert("declared_id".to_owned(), json!(declared));
    }
    value
}

fn cycles(components: &[components::Component], edges: &[Value]) -> Result<Value, String> {
    let index = components
        .iter()
        .enumerate()
        .map(|(slot, component)| (component.id.as_str(), slot))
        .collect::<std::collections::BTreeMap<_, _>>();
    let mut links = Vec::new();
    for edge in edges {
        let (Some(from), Some(to)) = (edge["from"].as_str(), edge["to"].as_str()) else {
            continue;
        };
        if let (Some(&from), Some(&to)) = (index.get(from), index.get(to)) {
            links.push(EdgeEndpoints::new(
                NodeIndex::new(u32::try_from(from).map_err(|e| e.to_string())?),
                NodeIndex::new(u32::try_from(to).map_err(|e| e.to_string())?),
            ));
        }
    }
    let topology = Topology::try_from_edges(components.len(), links).map_err(|e| e.to_string())?;
    let mut cyclic = strongly_connected_components_filtered(&topology, |_| true)
        .into_iter()
        .filter(|group| group.len() > 1)
        .map(|group| {
            let mut members = group
                .into_iter()
                .map(|node| components[node.index()].id.clone())
                .collect::<Vec<_>>();
            members.sort();
            members
        })
        .collect::<Vec<_>>();
    cyclic.sort();
    let cyclic_total = cyclic.len();
    let condensed = condensation_filtered(&topology, |_| true).map_err(|e| e.to_string())?;
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
