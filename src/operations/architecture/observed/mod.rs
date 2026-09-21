//! Observed folders, packages, and typed edges. This is not a style label
//! and not the starter contract.

mod components;
mod cycles;
mod edges;
mod packages;
mod summary;

use crate::engine::RepositoryState;
use crate::model::digest::sha3_256;
use crate::operations::{optional_str, optional_u64, reject_unknown_arguments};
use blazingly_json::{Value, json};

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
        &["detail", "max_results", "edge_cursor", "token_budget"],
    )?;
    let detail = optional_str(args, "detail")?.unwrap_or_else(|| {
        if args.get("max_results").is_some() || args.get("edge_cursor").is_some() {
            "full"
        } else {
            "summary"
        }
    });
    if !matches!(detail, "summary" | "full") {
        return Err("detail must be summary or full".to_owned());
    }
    if detail == "summary" {
        if args.get("max_results").is_some() || args.get("edge_cursor").is_some() {
            return Err("summary does not page edges; use detail=full".to_owned());
        }
        let full = facts(state, Some((usize::MAX, None)))?;
        let mut report = summary::from_full(&full);
        let budget = crate::operations::token_budget::requested(args)?;
        crate::operations::token_budget::fit(
            &mut report,
            budget,
            &[
                "/coupling",
                "/cycle_candidates/samples",
                "/components",
                "/packages",
                "/build/runners",
            ],
        );
        if report["token_budget"]["dropped_items"]
            .as_u64()
            .is_some_and(|dropped| dropped > 0)
        {
            report["status"] = json!("INCOMPLETE");
        }
        return Ok(report);
    }
    let max = optional_u64(args, "max_results")?.unwrap_or(200).min(500) as usize;
    if max == 0 {
        return Err("max_results must be at least 1".to_owned());
    }
    let cursor = optional_str(args, "edge_cursor")?;
    let mut report = facts(state, Some((max, cursor)))?;
    report["detail"] = json!("full");
    let budget = crate::operations::token_budget::requested(args)?;
    crate::operations::token_budget::fit(
        &mut report,
        budget,
        &[
            "/edges",
            "/internal_connectivity",
            "/cycles/cyclic_scc",
            "/cycles/condensation_edge_list",
            "/cycles/condensation_components",
            "/cycles/dag_generations",
            "/components",
            "/packages",
            "/build_topology/workspaces",
        ],
    );
    let offset = report["edge_offset"].as_u64().unwrap_or(0);
    let returned = report["edges"].as_array().map_or(0, Vec::len) as u64;
    let total = report["edges_total"].as_u64().unwrap_or(0);
    let next = offset.saturating_add(returned);
    report["edges_returned"] = json!(returned);
    report["edges_truncated"] = json!(next < total);
    report["next_edge_cursor"] = if next < total {
        json!(format!(
            "{}:{next}",
            report["analysis_id"].as_str().unwrap_or_default()
        ))
    } else {
        Value::Null
    };
    if report["token_budget"]["dropped_items"]
        .as_u64()
        .is_some_and(|dropped| dropped > 0)
    {
        report["status"] = json!("INCOMPLETE");
    }
    Ok(report)
}

pub(crate) fn memberships(state: &RepositoryState) -> Vec<(String, String)> {
    components::collect(state, None)
        .into_iter()
        .map(|component| (component.path, component.id))
        .collect()
}

pub(crate) fn root_component_id(state: &RepositoryState) -> String {
    components::root_id(state)
}

fn facts(state: &RepositoryState, page: Option<(usize, Option<&str>)>) -> Result<Value, String> {
    let capture = state.evidence().receipt();
    let (declaration, diagnostic) = match super::contract::load_optional(state) {
        Ok(value) => (value, None),
        Err(error) => (None, Some(error)),
    };
    let components = components::collect(state, declaration.as_ref());
    let relations = edges::collect(state, &components);
    let cycles = cycles::analyze(&components, &relations.cross)?;
    let build = crate::operations::build::model(state);
    let total = relations.cross.len();
    // A cursor belongs to these captured graph facts, not just a revision name.
    let identity = sha3_256(
        &blazingly_json::to_vec(&json!({
            "components": components.iter().map(component_json).collect::<Vec<_>>(),
            "edges": relations.cross,
            "internal_connectivity": relations.internal
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
    let returned = relations
        .cross
        .iter()
        .skip(start)
        .take(max)
        .cloned()
        .collect::<Vec<_>>();
    let next = start.saturating_add(returned.len());
    let truncated = next < total;
    Ok(json!({
        "kind": "observed",
        "analysis_id": identity,
        "model": "structural directory fallback and typed graph edges; not a style label or build-module claim",
        "view": "structural_directory_fallback.v1",
        "status": if diagnostic.is_some() || !capture.complete || truncated || cycles["cyclic_scc_truncated"] == true || cycles["condensation_truncated"] == true {"INCOMPLETE"} else {"COMPLETE"},
        "analysis": {"coverage": "BOUNDED_OBSERVATION", "closed_world": false,
                     "discovered_relations_evaluated": true,
                     "unknowns_total": usize::from(diagnostic.is_some()) + capture.excluded.len()},
        "input_capture": capture,
        "diagnostics": diagnostic.into_iter().collect::<Vec<_>>(),
        "packages": packages::collect(&build),
        "build_topology": crate::operations::build::architecture_topology(&build),
        "components": components.iter().map(component_json).collect::<Vec<_>>(),
        "edges": returned,
        "edge_offset": start,
        "edges_total": total,
        "edges_returned": next - start,
        "edges_truncated": truncated,
        "next_edge_cursor": truncated.then(|| format!("{identity}:{next}")),
        "internal_connectivity": relations.internal,
        "internal_connectivity_total": relations.internal.len(),
        "cycles": cycles
    }))
}

fn component_json(component: &components::Component) -> Value {
    let mut value = json!({
        "id": component.id,
        "entity_key": {"repository_id": component.repository_id,
                       "kind": "component_view", "canonical_relative_path": component.path,
                       "native_logical_key": "structural_directory_fallback.v1"},
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
