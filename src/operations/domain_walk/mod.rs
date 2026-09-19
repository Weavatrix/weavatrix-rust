mod graph;

use crate::engine::RepositoryState;
use blazingly_json::{Value, json};
use std::collections::BTreeSet;

pub(super) struct WalkSpec<'a> {
    pub start: &'a str,
    pub depth: usize,
    pub max_nodes: usize,
    pub offset: usize,
    pub relations: &'a [&'a str],
    pub port_kind: &'a str,
    pub owner_kinds: &'a [&'a str],
    pub incoming: bool,
    pub revision: &'a str,
}

pub(super) fn trace(state: &RepositoryState, spec: &WalkSpec<'_>) -> Value {
    let index = graph::Adjacency::build(state, spec.port_kind, spec.owner_kinds);
    let mut cycle = false;
    let mut steps = Vec::new();
    let mut reasons = BTreeSet::new();
    for relation in spec.relations {
        graph::walk_relation(
            &index,
            spec.start,
            spec.depth,
            relation,
            spec.incoming,
            spec.max_nodes,
            &mut steps,
            &mut reasons,
        );
        cycle |= graph::relation_has_cycle(&index, &steps, relation);
    }
    let total = steps.len();
    let end = spec.offset.saturating_add(spec.max_nodes).min(total);
    let page = if spec.offset > total {
        Vec::new()
    } else {
        steps[spec.offset..end].to_vec()
    };
    if end < total {
        reasons.insert("page");
    }
    json!({
        "start": spec.start,
        "steps": page,
        "cycle": cycle,
        "page": {
            "offset": spec.offset,
            "returned": page.len(),
            "total": total,
            "has_more": end < total,
            "next_cursor": (end < total).then(|| format!("v1:{end}:{}", spec.revision))
        },
        "bounds": {
            "truncated": !reasons.is_empty(),
            "found": total,
            "shown": page.len(),
            "reasons": reasons.into_iter().collect::<Vec<_>>(),
            "depth": spec.depth,
            "max_nodes": spec.max_nodes,
            "revision": spec.revision,
            "runtime": false
        }
    })
}

pub(super) fn page_offset_for(args: &Value, revision: &str) -> Result<usize, String> {
    let Some(cursor) = args.get("cursor").and_then(Value::as_str) else {
        return Ok(0);
    };
    let Some(rest) = cursor.strip_prefix("v1:") else {
        return Err(
            "cursor format is invalid; expected v1:<offset> or v1:<offset>:<revision>".to_owned(),
        );
    };
    let mut parts = rest.splitn(2, ':');
    let offset = parts
        .next()
        .ok_or_else(|| "cursor offset is invalid".to_owned())?
        .parse::<usize>()
        .map_err(|_| "cursor offset is invalid".to_owned())?;
    if let Some(bound) = parts.next()
        && bound != revision
    {
        return Err("cursor belongs to a different revision".to_owned());
    }
    Ok(offset)
}

pub(super) struct TraceRequest {
    pub label: String,
    pub depth: usize,
    pub max_nodes: usize,
}

pub(super) fn parse_trace(
    operation: &str,
    args: &Value,
    default_max_nodes: u64,
) -> Result<TraceRequest, String> {
    crate::operations::reject_unknown_arguments(
        operation,
        args,
        &["label", "depth", "max_nodes", "cursor", "direction"],
    )?;
    let label = crate::operations::arg_str(args, "label")?.to_owned();
    let depth = usize::try_from(crate::operations::optional_u64(args, "depth")?.unwrap_or(8))
        .map_err(|_| "depth is too large")?;
    if !(1..=32).contains(&depth) {
        return Err("depth must be between 1 and 32".to_owned());
    }
    let max_nodes = usize::try_from(
        crate::operations::optional_u64(args, "max_nodes")?.unwrap_or(default_max_nodes),
    )
    .map_err(|_| "max_nodes is too large")?;
    if max_nodes == 0 || max_nodes > 500 {
        return Err("max_nodes must be between 1 and 500".to_owned());
    }
    Ok(TraceRequest {
        label,
        depth,
        max_nodes,
    })
}

pub(super) fn incoming_arg(args: &Value) -> Result<bool, String> {
    match args.get("direction").and_then(Value::as_str) {
        None | Some("outgoing") => Ok(false),
        Some("incoming") => Ok(true),
        Some(other) => Err(format!(
            "direction must be outgoing or incoming, not {other}"
        )),
    }
}
