use crate::engine::RepositoryState;
use crate::operations::domain_walk::{self, WalkSpec};
use crate::operations::{arg_str, optional_u64, reject_unknown_arguments};
use blazingly_json::Value;

const RELATIONS: &[&str] = &["declared_architecture"];

pub(super) fn trace(state: &RepositoryState, args: &Value) -> Result<Value, String> {
    reject_unknown_arguments(
        "diagram_trace",
        args,
        &["label", "depth", "max_nodes", "cursor", "direction"],
    )?;
    let label = arg_str(args, "label")?;
    let depth = usize::try_from(optional_u64(args, "depth")?.unwrap_or(8))
        .map_err(|_| "depth is too large")?;
    if !(1..=32).contains(&depth) {
        return Err("depth must be between 1 and 32".to_owned());
    }
    let max_nodes = usize::try_from(optional_u64(args, "max_nodes")?.unwrap_or(100))
        .map_err(|_| "max_nodes is too large")?;
    if max_nodes == 0 || max_nodes > 500 {
        return Err("max_nodes must be between 1 and 500".to_owned());
    }
    let revision = state.snapshot().revision.as_str();
    let offset = domain_walk::page_offset_for(args, revision)?;
    let incoming = domain_walk::incoming_arg(args)?;
    let start = state.resolve_node(label)?;
    let start_node = state.node(start)?;
    let start_id = start_node.id.clone();
    let mut report = domain_walk::trace(
        state,
        &WalkSpec {
            start: start_id.as_str(),
            depth,
            max_nodes,
            offset,
            relations: RELATIONS,
            port_kind: "mermaid.group",
            owner_kinds: &["mermaid.element", "mermaid.diagram"],
            incoming,
            revision,
        },
    );
    if let Some(object) = report.as_object_mut() {
        object.insert("plane".to_owned(), blazingly_json::json!("declared_architecture"));
        object.insert("label".to_owned(), blazingly_json::json!(label));
    }
    Ok(report)
}
