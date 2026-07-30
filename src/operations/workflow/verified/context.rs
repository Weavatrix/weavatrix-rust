use super::model::ContextEvidence;
use crate::engine::RepositoryState;
use crate::operations::optional_u64;
use blazingly_json::{Value, json};
use std::collections::BTreeSet;
use weavatrix_graph::{Direction, EdgeKind, NodeIndex};

pub(super) fn build_context(
    state: &RepositoryState,
    phase: &str,
    task: &str,
    files: &[String],
    impact: &Value,
    args: &Value,
) -> Result<ContextEvidence, String> {
    let (retrieval, seeds) = if phase == "verify" && files.is_empty() {
        (
            json!({
                "selected": [],
                "max_symbols": 0,
                "model": "no changed files; no edit context is required"
            }),
            Vec::new(),
        )
    } else {
        retrieve_change_context(state, task, files, impact, args)?
    };
    let edit_contexts = edit_contexts(state, &seeds)?;
    let data_flow = data_flow_evidence(state, &seeds, args)?;
    Ok(ContextEvidence {
        retrieval,
        edit_contexts,
        data_flow,
    })
}

fn edit_contexts(state: &RepositoryState, seeds: &[NodeIndex]) -> Result<Vec<Value>, String> {
    seeds
        .iter()
        .filter_map(|seed| state.graph().node_at(*seed))
        .filter(|node| node.span.is_some())
        .map(|node| {
            crate::operations::source::context(
                state,
                &json!({
                    "label": node.id.as_str(),
                    "max_related": 8,
                    "context_lines": 4
                }),
            )
            .map(|evidence| {
                json!({"symbol": node.id.as_str(), "status": "COMPLETE", "evidence": evidence})
            })
        })
        .collect()
}

fn retrieve_change_context(
    state: &RepositoryState,
    task: &str,
    files: &[String],
    impact: &Value,
    args: &Value,
) -> Result<(Value, Vec<NodeIndex>), String> {
    let max = usize::try_from(optional_u64(args, "max_symbols")?.unwrap_or(12))
        .map_err(|_| "max_symbols is too large".to_owned())?
        .clamp(1, 50);
    let changed = files.iter().map(String::as_str).collect::<BTreeSet<_>>();
    let terms = task
        .split(|character: char| !character.is_alphanumeric() && character != '_')
        .filter(|term| term.len() >= 3)
        .map(str::to_ascii_lowercase)
        .collect::<BTreeSet<_>>();
    let mut selected = Vec::new();
    let mut seen = BTreeSet::new();
    let mut push = |index: NodeIndex| {
        if selected.len() < max && seen.insert(index) {
            selected.push(index);
        }
    };
    for id in impact["impacted_nodes"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|node| node["id"].as_str())
    {
        if let Some(index) = state.graph().node_index(id) {
            push(index);
        }
    }
    for (slot, node) in state.graph().nodes().iter().enumerate() {
        if node
            .span
            .as_ref()
            .is_some_and(|span| changed.contains(span.file.as_str()))
        {
            push(NodeIndex::new(u32::try_from(slot).unwrap_or(u32::MAX)));
        }
    }
    for (slot, node) in state.graph().nodes().iter().enumerate() {
        let text = format!(
            "{} {}",
            node.label,
            node.span.as_ref().map_or("", |span| span.file.as_str())
        )
        .to_ascii_lowercase();
        if !terms.is_empty() && terms.iter().any(|term| text.contains(term)) {
            push(NodeIndex::new(u32::try_from(slot).unwrap_or(u32::MAX)));
        }
    }
    let evidence = selected
        .iter()
        .filter_map(|index| state.graph().node_at(*index))
        .collect::<Vec<_>>();
    Ok((
        json!({
            "selected": evidence,
            "max_symbols": max,
            "model": "changed declarations, graph blast radius and task-term matches"
        }),
        selected,
    ))
}

fn data_flow_evidence(
    state: &RepositoryState,
    seeds: &[NodeIndex],
    args: &Value,
) -> Result<Value, String> {
    let depth = usize::try_from(optional_u64(args, "data_flow_depth")?.unwrap_or(2))
        .map_err(|_| "data_flow_depth is too large".to_owned())?
        .clamp(1, 3);
    let max = usize::try_from(optional_u64(args, "max_data_flow_edges")?.unwrap_or(30))
        .map_err(|_| "max_data_flow_edges is too large".to_owned())?
        .clamp(1, 60);
    let relations = [EdgeKind::Calls.as_str(), EdgeKind::References.as_str()]
        .into_iter()
        .map(str::to_owned)
        .collect::<BTreeSet<_>>();
    let (_, traversed) = crate::operations::graph::traverse(
        state,
        seeds.to_vec(),
        depth,
        max.saturating_mul(2).saturating_add(seeds.len()),
        Direction::Both,
        false,
        Some(&relations),
    );
    let total = traversed.len();
    let edges = traversed
        .into_iter()
        .filter_map(|index| state.graph().edge_at(index))
        .take(max)
        .collect::<Vec<_>>();
    Ok(json!({
        "status": "COMPLETE",
        "model": "bounded call/reference graph evidence; not CFG or taint analysis",
        "depth": depth,
        "edges": edges,
        "total_edges": total,
        "capped": total > max
    }))
}
