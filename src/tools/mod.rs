mod architecture;
mod catalog;
mod catalog_schema;
mod graph;
mod graph_trace;
mod graph_walk;
mod health;
mod health_coverage;
mod health_dependencies;
mod health_manifests;
mod health_runtime;
mod history;
#[cfg(feature = "git")]
mod history_analytics;
#[cfg(feature = "git")]
mod history_diff;
mod memory;
mod semantic;
mod source;
mod transport_contracts;
mod vector;
mod workflow;

pub use catalog::{ToolDefinition, catalog, catalog_for_profile};

use crate::Weavatrix;
use blazingly_json::{Value, json};

/// Executes one bounded read-only repository tool.
///
/// # Errors
///
/// Returns invalid arguments, unavailable optional capabilities, or analysis
/// failures without mutating repository source.
#[allow(clippy::needless_pass_by_value)]
pub fn call(weavatrix: &mut Weavatrix, name: &str, arguments: Value) -> Result<Value, String> {
    if name == "trace_api_contract" {
        return workflow::trace_api_cached(weavatrix, &arguments);
    }
    let state = weavatrix.state();
    match name {
        "graph_stats" => Ok(graph::stats(state)),
        "get_node" => graph::get_node(state, &arguments),
        "get_neighbors" => graph::neighbors(state, &arguments),
        "query_graph" => graph::query(state, &arguments),
        "god_nodes" => Ok(graph::hubs(state, &arguments)),
        "shortest_path" => graph::path(state, &arguments),
        "get_dependents" => graph::dependents(state, &arguments),
        "change_impact" => workflow::change_impact(state, &arguments),
        "git_history" => history::history(state, &arguments),
        "cross_repo_git" => history::cross_repo(state, &arguments),
        "verified_change" => workflow::verified_change(state, &arguments),
        "get_community" | "list_communities" => graph::communities(state, &arguments),
        "search_code" => source::search(state, &arguments),
        "read_source" => source::read_source(state, &arguments),
        "inspect_symbol" => source::inspect(state, &arguments),
        "context_bundle" => source::context(state, &arguments),
        "find_duplicates" => health::duplicates(state, &arguments),
        "find_dead_code" => health::dead_code(state, &arguments),
        "run_audit" => health::audit(state, &arguments),
        "coverage_map" => health::coverage(state, &arguments),
        "hot_path_review" => health::hot_paths(state, &arguments),
        "module_map" => Ok(graph::module_map(state, &arguments)),
        "list_endpoints" => graph::endpoints(state, &arguments),
        "trace_endpoint" => graph_trace::endpoint(state, &arguments),
        "graph_diff" => history::graph_diff(state, &arguments),
        "get_architecture_contract" => architecture::contract(state, &arguments),
        "prepare_change" => architecture::prepare(state, &arguments),
        "verify_architecture" => architecture::verify(state),
        "explain_architecture_violation" => architecture::explain(state, &arguments),
        "propose_architecture_exception" => architecture::propose_exception(state, &arguments),
        "semantic_link" => semantic::semantic_link(state, &arguments),
        "vector_search" => vector::search(&arguments),
        "seo_link_suggestions" => semantic::seo_links(state, &arguments),
        "memory_context" => memory::context(state, &arguments),
        "rebuild_graph" => {
            let before = graph::stats(state);
            weavatrix.rebuild().map_err(|error| error.to_string())?;
            Ok(json!({"before": before, "after": graph::stats(weavatrix.state())}))
        }
        "open_repo" => {
            let path = arg_str(&arguments, "path")?.to_owned();
            let should_build = arg_bool(&arguments, "build").unwrap_or(true);
            let graph_built = weavatrix
                .open_repository_with_build(&path, should_build)
                .map_err(|error| error.to_string())?;
            Ok(json!({
                "repository": weavatrix.state().root(),
                "built": graph_built,
                "graph": graph::stats(weavatrix.state())
            }))
        }
        "list_known_repos" => Ok(json!({
            "repositories": weavatrix.known_roots().collect::<Vec<_>>()
        })),
        _ => Err(format!("unknown tool: {name}")),
    }
}

fn arg_value<'value, T>(
    args: &'value Value,
    key: &str,
    expected: &str,
    extract: impl FnOnce(&'value Value) -> Option<T>,
) -> Result<T, String> {
    args.get(key)
        .and_then(extract)
        .ok_or_else(|| format!("{key} must be {expected}"))
}

pub(crate) fn arg_str<'value>(args: &'value Value, key: &str) -> Result<&'value str, String> {
    arg_value(args, key, "a string", Value::as_str)
}

pub(crate) fn arg_u64(args: &Value, key: &str) -> Result<u64, String> {
    arg_value(args, key, "a non-negative integer", Value::as_u64)
}

pub(crate) fn arg_bool(args: &Value, key: &str) -> Result<bool, String> {
    arg_value(args, key, "a boolean", Value::as_bool)
}

pub(crate) fn optional_str<'value>(
    args: &'value Value,
    key: &str,
) -> Result<Option<&'value str>, String> {
    args.get(key)
        .map(|value| {
            value
                .as_str()
                .ok_or_else(|| format!("{key} must be a string"))
        })
        .transpose()
}

pub(crate) fn optional_u64(args: &Value, key: &str) -> Result<Option<u64>, String> {
    args.get(key)
        .map(|value| {
            value
                .as_u64()
                .ok_or_else(|| format!("{key} must be a non-negative integer"))
        })
        .transpose()
}

pub(crate) fn optional_bool(args: &Value, key: &str) -> Result<Option<bool>, String> {
    args.get(key)
        .map(|value| {
            value
                .as_bool()
                .ok_or_else(|| format!("{key} must be a boolean"))
        })
        .transpose()
}

#[cfg(any(feature = "semantic", feature = "vector"))]
fn vector_values(value: &Value, array_error: &str) -> Result<Vec<f32>, String> {
    value
        .as_array()
        .ok_or_else(|| array_error.to_owned())?
        .iter()
        .map(|value| {
            let value = value
                .as_f64()
                .filter(|value| value.is_finite())
                .ok_or_else(|| "vector value must be finite".to_owned())?;
            if !(f64::from(f32::MIN)..=f64::from(f32::MAX)).contains(&value) {
                return Err("vector value is outside finite f32 range".to_owned());
            }
            value
                .to_string()
                .parse::<f32>()
                .map_err(|error| format!("invalid vector value: {error}"))
        })
        .collect()
}

/// The repository path a node's evidence comes from, if any.
pub(crate) fn node_path(node: &weavatrix_graph::Node) -> Option<&str> {
    node.span
        .as_ref()
        .map(|span| span.file.as_str())
        .or_else(|| (node.kind == weavatrix_graph::NodeKind::File).then_some(node.label.as_str()))
}

/// Whether a node belongs in a production-first answer.
///
/// Every tool whose schema offers `include_classified` or `include_tests` must
/// route through this, otherwise the parameter is advertised and ignored and
/// the answer silently mixes test and generated evidence into production
/// review.
pub(crate) fn node_is_visible(state: &crate::RepositoryState, slot: usize, args: &Value) -> bool {
    let index = weavatrix_graph::NodeIndex::new(u32::try_from(slot).unwrap_or(u32::MAX));
    let Some(node) = state.graph().node_at(index) else {
        return true;
    };
    if node_path(node).is_some() {
        return evidence_node_is_visible(node, args);
    }
    // Domain nodes such as endpoints, tables and topics carry no span: they are
    // classified by the files that declare them, so a route declared only in a
    // test is not part of a production-first answer.
    let mut declared = false;
    for edge in state.graph().incoming_at(index) {
        let Some(source) = state.graph().node(edge.source.as_str()) else {
            continue;
        };
        if node_path(source).is_none() {
            continue;
        }
        declared = true;
        if evidence_node_is_visible(source, args) {
            return true;
        }
    }
    // Repository and package nodes have no declaring file; keep them rather
    // than hide evidence.
    !declared
}

fn evidence_node_is_visible(node: &weavatrix_graph::Node, args: &Value) -> bool {
    if matches!(
        node.attributes.get("test_only"),
        Some(weavatrix_graph::AttributeValue::Bool(true))
    ) {
        return args.get("include_tests").and_then(Value::as_bool) == Some(true);
    }
    node_path(node).is_none_or(|path| health::path_is_visible(path, args))
}
