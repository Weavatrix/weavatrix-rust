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
        cycle |= graph::relation_has_cycle(&steps, relation);
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

pub(super) fn incoming_arg(args: &Value) -> Result<bool, String> {
    match args.get("direction").and_then(Value::as_str) {
        None | Some("outgoing") => Ok(false),
        Some("incoming") => Ok(true),
        Some(other) => Err(format!(
            "direction must be outgoing or incoming, not {other}"
        )),
    }
}
