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
        "trace_api_contract" => workflow::trace_api(state, &arguments),
        "get_community" | "list_communities" => graph::communities(state, &arguments),
        "search_code" => source::search(state, &arguments),
        "read_source" => source::read_source(state, &arguments),
        "inspect_symbol" => source::inspect(state, &arguments),
        "context_bundle" => source::context(state, &arguments),
        "find_duplicates" => health::duplicates(state, &arguments),
        "find_dead_code" => Ok(health::dead_code(state, &arguments)),
        "run_audit" => Ok(health::audit(state, &arguments)),
        "coverage_map" => Ok(health::coverage(state, &arguments)),
        "hot_path_review" => Ok(health::hot_paths(state, &arguments)),
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
            weavatrix
                .open_repository(&path)
                .map_err(|error| error.to_string())?;
            Ok(json!({"repository": path, "graph": graph::stats(weavatrix.state())}))
        }
        "list_known_repos" => Ok(json!({
            "repositories": weavatrix.known_roots().collect::<Vec<_>>()
        })),
        _ => Err(format!("unknown tool: {name}")),
    }
}

pub(crate) fn arg_str<'value>(args: &'value Value, key: &str) -> Result<&'value str, String> {
    args.get(key)
        .and_then(Value::as_str)
        .ok_or_else(|| format!("{key} must be a string"))
}

pub(crate) fn arg_u64(args: &Value, key: &str) -> Result<u64, String> {
    args.get(key)
        .and_then(Value::as_u64)
        .ok_or_else(|| format!("{key} must be a non-negative integer"))
}

#[cfg(feature = "semantic")]
pub(crate) fn arg_f64(args: &Value, key: &str) -> Result<f64, String> {
    args.get(key)
        .and_then(Value::as_f64)
        .ok_or_else(|| format!("{key} must be a number"))
}

pub(crate) fn arg_bool(args: &Value, key: &str) -> Result<bool, String> {
    args.get(key)
        .and_then(Value::as_bool)
        .ok_or_else(|| format!("{key} must be a boolean"))
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
    if let Some(path) = node_path(node) {
        return health::path_is_visible(path, args);
    }
    // Domain nodes such as endpoints, tables and topics carry no span: they are
    // classified by the files that declare them, so a route declared only in a
    // test is not part of a production-first answer.
    let mut declared = false;
    for edge in state.graph().incoming_at(index) {
        let Some(source) = state.graph().node(edge.source.as_str()) else {
            continue;
        };
        let Some(path) = node_path(source) else {
            continue;
        };
        declared = true;
        if health::path_is_visible(path, args) {
            return true;
        }
    }
    // Repository and package nodes have no declaring file; keep them rather
    // than hide evidence.
    !declared
}
