use super::paths::{path_is_in_scope, requested_path_scope};
use crate::engine::RepositoryState;
use crate::operations::{optional_bool, optional_u64};
use blazingly_json::{Value, json};
use weavatrix_graph::NodeIndex;

pub(in crate::operations) fn hot_paths(
    state: &RepositoryState,
    args: &Value,
) -> Result<Value, String> {
    let top = usize::try_from(optional_u64(args, "top_n")?.unwrap_or(20))
        .map_err(|_| "top_n is too large".to_owned())?;
    let path_scope = requested_path_scope(args)?;
    let _ = optional_bool(args, "include_tests")?;
    let _ = optional_bool(args, "include_classified")?;
    let mut ranked = state
        .graph()
        .nodes()
        .iter()
        .enumerate()
        .filter(|(_, node)| {
            path_is_in_scope(
                crate::operations::node_path(node).unwrap_or_default(),
                path_scope.as_deref(),
            )
        })
        .filter(|(slot, _)| crate::operations::node_is_visible(state, *slot, args))
        .filter_map(|(slot, node)| {
            let span = node.span.as_ref()?;
            let lines = span
                .end
                .line
                .saturating_sub(span.start.line)
                .saturating_add(1);
            let index = NodeIndex::new(u32::try_from(slot).unwrap_or(u32::MAX));
            let degree = state
                .graph()
                .in_degree(index)
                .unwrap_or(0)
                .saturating_add(state.graph().out_degree(index).unwrap_or(0));
            let score = u64::from(lines)
                .saturating_add(u64::try_from(degree).unwrap_or(u64::MAX).saturating_mul(5));
            Some((score, lines, degree, node))
        })
        .collect::<Vec<_>>();
    ranked.sort_unstable_by(|left, right| {
        right
            .0
            .cmp(&left.0)
            .then_with(|| left.3.id.cmp(&right.3.id))
    });
    Ok(json!({
        "candidates": ranked.into_iter().take(top).map(|(score, lines, degree, node)| {
            json!({"node": node, "score": score, "source_lines": lines, "graph_degree": degree})
        }).collect::<Vec<_>>(),
        "model": "source span plus graph fan-in/fan-out; not profiler data"
    }))
}
