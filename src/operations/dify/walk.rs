use crate::engine::RepositoryState;
use crate::operations::domain_walk::{self, WalkSpec};
use blazingly_json::Value;

const TRACE_PAGE: u64 = 100;
const RELATIONS: &[&str] = &[
    "flows_to",
    "depends_on_output",
    "reads_variable",
    "writes_variable",
    "binds_input",
    "uses_model",
    "invokes_tool",
    "queries_knowledge",
];

pub(super) fn trace(state: &RepositoryState, args: &Value) -> Result<Value, String> {
    let request = domain_walk::parse_trace("dify_trace", args, TRACE_PAGE)?;
    let revision = state.snapshot().revision.as_str();
    let offset = domain_walk::page_offset_for(args, revision)?;
    let incoming = domain_walk::incoming_arg(args)?;
    let start = state.resolve_node(&request.label)?;
    let start_id = state.node(start)?.id.clone();
    let mut report = domain_walk::trace(
        state,
        &WalkSpec {
            start: start_id.as_str(),
            depth: request.depth,
            max_nodes: request.max_nodes,
            offset,
            relations: RELATIONS,
            port_kind: "dify.port",
            owner_kinds: &["dify.node", "dify.app"],
            incoming,
            revision,
        },
    );
    if let Some(object) = report.as_object_mut() {
        object.insert("label".to_owned(), blazingly_json::json!(request.label));
    }
    Ok(report)
}
