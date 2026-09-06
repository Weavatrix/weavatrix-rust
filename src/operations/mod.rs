mod architecture;
mod args;
mod build;
mod catalog;
mod graph;
mod health;
mod history;
mod memory;
mod occurrence;
mod perf;
mod semantic;
mod source;
mod syntax;
mod token_budget;
mod transport_contracts;
mod vector;
mod visibility;
mod workflow;

pub use catalog::{ToolDefinition, ToolProfile, catalog, catalog_for_profile};
pub(crate) use catalog::{reject_unknown_arguments, require_valid_output_format};

pub(crate) use args::{
    arg_bool, arg_str, arg_u64, optional_bool, optional_str, optional_u64, require_graph_precision,
};
pub(crate) use visibility::{node_is_visible, node_path};

use crate::engine::Weavatrix;
use blazingly_json::{Value, json};

/// Executes one bounded read-only repository tool.
///
/// # Errors
///
/// Returns invalid arguments, unavailable optional capabilities, or analysis
/// failures without mutating repository source.
#[allow(clippy::needless_pass_by_value)]
pub fn call(weavatrix: &mut Weavatrix, name: &str, arguments: Value) -> Result<Value, String> {
    weavatrix.prepare();
    require_valid_output_format(&arguments)?;
    expect_repository(weavatrix, &arguments)?;
    let mut report = dispatch(weavatrix, name, &arguments)?;
    // A budget an operation cannot apply is reported, not refused: the answer
    // itself is never withheld.
    token_budget::annotate_unapplied(name, &arguments, &mut report)?;
    attach_repository_context(weavatrix, &mut report);
    Ok(report)
}

/// Fails fast when the caller names the repository it expects and this
/// process is targeting a different one. A stateful server that silently
/// answers about the previously opened repository produces confidently wrong
/// evidence, which is worse than an error.
fn expect_repository(weavatrix: &Weavatrix, args: &Value) -> Result<(), String> {
    let Some(expected) = optional_str(args, "expected_repository")? else {
        return Ok(());
    };
    let active = weavatrix.state().root();
    let canonical = std::path::Path::new(expected).canonicalize().ok();
    if canonical.as_deref() == Some(active) {
        return Ok(());
    }
    let folder = |path: &std::path::Path| {
        path.file_name()
            .map(|name| name.to_string_lossy().to_ascii_lowercase())
    };
    if folder(std::path::Path::new(expected)) == folder(active) {
        return Ok(());
    }
    Err(format!(
        "active repository is {}, not the expected {expected}; call open_repo first",
        active.display()
    ))
}

/// Every answer names the repository, revision and graph age it came from, so
/// a caller can detect a stale graph or a wrong active repository without a
/// second round trip.
fn attach_repository_context(weavatrix: &Weavatrix, report: &mut Value) {
    let state = weavatrix.state();
    let Some(object) = report.as_object_mut() else {
        return;
    };
    object.insert(
        "repository_context".to_owned(),
        json!({
            "root": state.root(),
            "scan_revision": state.snapshot().revision,
            "git_head": crate::engine::git_head(state.root()),
            "graph_age_seconds": state.graph_age_seconds()
        }),
    );
}

fn dispatch(weavatrix: &mut Weavatrix, name: &str, arguments: &Value) -> Result<Value, String> {
    if name == "trace_api_contract" {
        return workflow::trace_api_cached(weavatrix, arguments);
    }
    let state = weavatrix.state();
    match name {
        "graph_stats" => graph::stats(state, arguments),
        "get_node" => graph::get_node(state, arguments),
        "get_neighbors" => graph::neighbors(state, arguments),
        "query_graph" => graph::query(state, arguments),
        "god_nodes" => Ok(graph::hubs(state, arguments)),
        "shortest_path" => graph::path(state, arguments),
        "get_dependents" => graph::dependents(state, arguments),
        "change_impact" => workflow::change_impact(state, arguments),
        "map_stacktrace" => workflow::map_stacktrace(state, arguments),
        "select_tests" => workflow::select_tests(state, arguments),
        "git_history" => history::history(state, arguments),
        "git_read_blob" => history::read_blob(state, arguments),
        "cross_repo_git" => history::cross_repo(state, arguments),
        "verified_change" => workflow::verified_change(state, arguments),
        "get_community" | "list_communities" => graph::communities(state, arguments),
        "search_code" => source::search(state, arguments),
        "read_source" => source::read_source(state, arguments),
        "inspect_symbol" => source::inspect(state, arguments),
        "go_to_definition" => occurrence::go_to_definition(state, arguments),
        "find_references" => occurrence::find_references(state, arguments),
        "context_bundle" => source::context(state, arguments),
        "find_duplicates" => health::duplicates(state, arguments),
        "find_dead_code" => health::dead_code(state, arguments),
        "run_audit" => health::audit(state, arguments),
        "coverage_map" => health::coverage(state, arguments),
        "hot_path_review" => health::hot_paths(state, arguments),
        "perf_attribution" => perf::attribution(state, arguments),
        "module_map" => graph::module_map(state, arguments),
        "build_graph" => build::build_graph(state, arguments),
        "list_endpoints" => graph::endpoints(state, arguments),
        "trace_endpoint" => graph::trace_endpoint(state, arguments),
        "graph_diff" => history::graph_diff(state, arguments),
        "get_architecture_contract" => architecture::contract(state, arguments),
        "prepare_change" => architecture::prepare(state, arguments),
        "verify_architecture" => architecture::verify(state),
        "verify_capabilities" => architecture::verify_capabilities(state, arguments),
        "explain_architecture_violation" => architecture::explain(state, arguments),
        "propose_architecture_exception" => architecture::propose_exception(state, arguments),
        "semantic_link" => semantic::semantic_link(state, arguments),
        "vector_search" => vector::search(arguments),
        "seo_link_suggestions" => semantic::seo_links(state, arguments),
        "memory_context" => memory::context(state, arguments),
        "rebuild_graph" => {
            let before = graph::stats(state, arguments)?;
            weavatrix.rebuild().map_err(|error| error.to_string())?;
            Ok(json!({"before": before, "after": graph::stats(weavatrix.state(), arguments)?}))
        }
        "open_repo" => {
            let path = arg_str(arguments, "path")?.to_owned();
            let should_build = arg_bool(arguments, "build").unwrap_or(true);
            let graph_built = weavatrix
                .open_repository_with_build(&path, should_build)
                .map_err(|error| error.to_string())?;
            Ok(json!({
                "repository": weavatrix.state().root(),
                "built": graph_built,
                "graph": graph::stats(weavatrix.state(), arguments)?
            }))
        }
        "list_known_repos" => Ok(json!({
            "repositories": weavatrix.known_roots().collect::<Vec<_>>(),
        })),
        _ => Err(format!("unknown tool: {name}")),
    }
}
