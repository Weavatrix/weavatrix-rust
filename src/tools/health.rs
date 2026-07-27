use crate::RepositoryState;
#[cfg(feature = "clone")]
use crate::tools::arg_str;
use crate::tools::arg_u64;
use serde_json::{Value, json};
use std::collections::BTreeMap;
use weavatrix_graph::{EdgeKind, NodeKind, strongly_connected_components};

#[cfg(feature = "clone")]
pub fn duplicates(state: &RepositoryState, args: &Value) -> Result<Value, String> {
    use weavatrix_clone::{
        CloneConfig, CloneDetector, DetectionMode, RepositoryCloneDetector, Similarity,
    };

    let mode = match arg_str(args, "mode").unwrap_or("near_miss") {
        "strict" | "exact" => DetectionMode::Exact,
        "renamed" => DetectionMode::Renamed,
        _ => DetectionMode::NearMiss,
    };
    let min_tokens = usize::try_from(arg_u64(args, "min_tokens").unwrap_or(24))
        .map_err(|_| "min_tokens is too large")?;
    let percent = arg_u64(args, "min_similarity").unwrap_or(80).min(100);
    let detector = CloneDetector::new(CloneConfig {
        mode,
        min_tokens,
        min_similarity: Similarity::from_permille(u16::try_from(percent * 10).unwrap_or(1_000)),
        ..CloneConfig::default()
    })
    .map_err(|error| error.to_string())?;
    let report = RepositoryCloneDetector::new(detector)
        .detect(state.root())
        .map_err(|error| error.to_string())?;
    let top = usize::try_from(arg_u64(args, "top_n").unwrap_or(15)).unwrap_or(15);
    Ok(json!({
        "families": report.families.into_iter().take(top).map(|family| json!({
            "id": family.id,
            "members": family.members.into_iter()
                .map(|location| location_json(&location)).collect::<Vec<_>>(),
            "pairs": family.pair_ids
        })).collect::<Vec<_>>(),
        "pairs": report.pairs.into_iter().take(top).map(|pair| json!({
            "id": pair.id,
            "kind": format!("{:?}", pair.kind).to_ascii_lowercase(),
            "similarity_percent": pair.similarity.percent(),
            "left": location_json(&pair.left),
            "right": location_json(&pair.right),
            "evidence": {
                "strict_equal": pair.evidence.strict_equal,
                "renamed_equal": pair.evidence.renamed_equal,
                "edit_distance": pair.evidence.edit_distance,
                "compared_tokens": pair.evidence.compared_tokens
            }
        })).collect::<Vec<_>>(),
        "statistics": {
            "source_files": report.statistics.source_files,
            "source_tokens": report.statistics.source_tokens,
            "candidate_pairs": report.statistics.candidate_pairs,
            "verified_pairs": report.statistics.verified_pairs
        }
    }))
}

#[cfg(feature = "clone")]
fn location_json(location: &weavatrix_clone::CloneLocation) -> Value {
    json!({
        "fragment_id": location.fragment_id,
        "path": location.path,
        "start_line": location.span.start_line,
        "end_line": location.span.end_line
    })
}

#[cfg(not(feature = "clone"))]
pub fn duplicates(_state: &RepositoryState, _args: &Value) -> Result<Value, String> {
    Err("clone capability is not compiled".to_owned())
}

pub fn dead_code(state: &RepositoryState, args: &Value) -> Value {
    let top = usize::try_from(arg_u64(args, "top_n").unwrap_or(30)).unwrap_or(30);
    let candidates = state
        .graph()
        .nodes()
        .iter()
        .enumerate()
        .filter(|(_, node)| {
            matches!(
                node.kind,
                NodeKind::File
                    | NodeKind::Function
                    | NodeKind::Method
                    | NodeKind::Struct
                    | NodeKind::Enum
                    | NodeKind::Trait
            )
        })
        .filter_map(|(slot, node)| {
            let index =
                weavatrix_graph::NodeIndex::new(u32::try_from(slot).unwrap_or(u32::MAX));
            let references = state
                .graph()
                .incoming_at(index)
                .filter(|edge| edge.kind != EdgeKind::Contains)
                .count();
            (references == 0).then(|| {
                json!({
                    "node": node,
                    "confidence": if node.kind == NodeKind::File {"low"} else {"medium"},
                    "reason": "no incoming static call/import/reference evidence",
                    "caveat": "framework, reflection, public API, runtime and generated use may be invisible"
                })
            })
        })
        .take(top)
        .collect::<Vec<_>>();
    json!({"candidates": candidates, "verdict": "REVIEW_ONLY"})
}

pub fn audit(state: &RepositoryState, args: &Value) -> Value {
    let max = usize::try_from(arg_u64(args, "max_findings").unwrap_or(30)).unwrap_or(30);
    let cycles = strongly_connected_components(state.graph())
        .into_iter()
        .filter(|component| component.len() > 1)
        .take(max)
        .map(|component| {
            component
                .into_iter()
                .filter_map(|index| state.graph().node_at(index))
                .map(|node| node.id.as_str())
                .collect::<Vec<_>>()
        })
        .collect::<Vec<_>>();
    let mut language_counts = BTreeMap::<String, u64>::new();
    for node in state.graph().nodes() {
        if let Some(language) = &node.language {
            *language_counts.entry(language.clone()).or_default() += 1;
        }
    }
    json!({
        "status": if state.snapshot().diagnostics.is_empty() {"PASS"} else {"REVIEW"},
        "findings": state.snapshot().diagnostics.iter().take(max).collect::<Vec<_>>(),
        "cycles": cycles,
        "languages": language_counts,
        "capability_matrix": state.snapshot().capabilities,
        "dependency_report": super::health_dependencies::report(state, max),
        "completeness": {
            "structure": "PARTIAL_LANGUAGE_AWARE",
            "dependencies": "PARTIAL_MANIFEST_AWARE",
            "runtime": "NOT_AVAILABLE",
            "advisories": "NOT_AVAILABLE",
            "malware": "NOT_AVAILABLE"
        }
    })
}

pub fn hot_paths(state: &RepositoryState, args: &Value) -> Value {
    let top = usize::try_from(arg_u64(args, "top_n").unwrap_or(20)).unwrap_or(20);
    let mut ranked = state
        .graph()
        .nodes()
        .iter()
        .enumerate()
        .filter_map(|(slot, node)| {
            let span = node.span.as_ref()?;
            let lines = span
                .end
                .line
                .saturating_sub(span.start.line)
                .saturating_add(1);
            let index = weavatrix_graph::NodeIndex::new(u32::try_from(slot).unwrap_or(u32::MAX));
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
    json!({
        "candidates": ranked.into_iter().take(top).map(|(score, lines, degree, node)| {
            json!({"node": node, "score": score, "source_lines": lines, "graph_degree": degree})
        }).collect::<Vec<_>>(),
        "model": "source span plus graph fan-in/fan-out; not profiler data"
    })
}

pub fn coverage(state: &RepositoryState, _args: &Value) -> Value {
    super::health_coverage::coverage(state)
}
