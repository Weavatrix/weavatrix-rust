use crate::RepositoryState;
#[cfg(feature = "clone")]
use crate::tools::arg_str;
use crate::tools::arg_u64;
use serde_json::{Value, json};
use std::collections::BTreeMap;
use weavatrix_graph::{EdgeKind, NodeKind, strongly_connected_components};

#[cfg(feature = "clone")]
#[allow(clippy::too_many_lines)]
pub fn duplicates(state: &RepositoryState, args: &Value) -> Result<Value, String> {
    use weavatrix_clone::{
        CloneConfig, CloneDetector, DetectionMode, RepositoryCloneDetector, Similarity,
    };

    let mode = match arg_str(args, "mode").unwrap_or("near_miss") {
        "strict" | "exact" => DetectionMode::Exact,
        "renamed" => DetectionMode::Renamed,
        _ => DetectionMode::NearMiss,
    };
    let min_tokens = usize::try_from(arg_u64(args, "min_tokens").unwrap_or(50))
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
    let include_tests = args.get("include_tests").and_then(Value::as_bool) == Some(true);
    let include_classified = args.get("include_classified").and_then(Value::as_bool) == Some(true);
    let visible = |path: &str| {
        let class = path_class(path);
        match class {
            PathClass::Product => true,
            PathClass::Test => include_tests,
            PathClass::Classified => include_classified,
        }
    };
    let mut suppressed_families = 0_usize;
    let mut families = report
        .families
        .into_iter()
        .filter(|family| {
            let keep = family.members.iter().any(|member| visible(&member.path));
            suppressed_families += usize::from(!keep);
            keep
        })
        .collect::<Vec<_>>();
    families.sort_by_key(|family| {
        core::cmp::Reverse(
            family
                .members
                .iter()
                .map(|member| {
                    usize::try_from(member.span.end_line.saturating_sub(member.span.start_line))
                        .unwrap_or(0)
                        + 1
                })
                .sum::<usize>(),
        )
    });
    let mut suppressed_pairs = 0_usize;
    let mut pairs = report
        .pairs
        .into_iter()
        .filter(|pair| {
            let keep = visible(&pair.left.path) || visible(&pair.right.path);
            suppressed_pairs += usize::from(!keep);
            keep
        })
        .collect::<Vec<_>>();
    pairs.sort_by_key(|pair| core::cmp::Reverse(pair.evidence.compared_tokens));
    let include_boilerplate =
        args.get("include_boilerplate").and_then(Value::as_bool) == Some(true);
    let include_declarative =
        args.get("include_declarative").and_then(Value::as_bool) == Some(true);
    let mut sources = std::collections::HashMap::<String, Vec<String>>::new();
    let mut suppressed_boilerplate = 0_usize;
    let mut suppressed_declarative = 0_usize;
    let families = families
        .into_iter()
        .filter(|family| {
            if !include_boilerplate
                && family
                    .members
                    .iter()
                    .all(|member| is_boilerplate(&member.path))
            {
                suppressed_boilerplate += 1;
                return false;
            }
            if !include_declarative
                && family.members.iter().all(|member| {
                    !has_control_flow(
                        state.root(),
                        &member.path,
                        member.span.start_line,
                        member.span.end_line,
                        &mut sources,
                    )
                })
            {
                suppressed_declarative += 1;
                return false;
            }
            true
        })
        .collect::<Vec<_>>();
    Ok(json!({
        "families": families.iter().take(top).map(|family| json!({
            "id": family.id,
            "members": family.members.iter()
                .map(location_json).collect::<Vec<_>>(),
            "pairs": family.pair_ids
        })).collect::<Vec<_>>(),
        "pairs": pairs.iter().take(top).map(|pair| json!({
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
        "suppressed": {
            "families": suppressed_families,
            "pairs": suppressed_pairs,
            "boilerplate_families": suppressed_boilerplate,
            "declarative_families": suppressed_declarative,
            "detail": "test/classified evidence, router/handler boilerplate and immutable declarative catalogs are suppressed by default; pass include_tests, include_classified, include_boilerplate or include_declarative to inspect them"
        },
        "statistics": {
            "source_files": report.statistics.source_files,
            "source_tokens": report.statistics.source_tokens,
            "candidate_pairs": report.statistics.candidate_pairs,
            "verified_pairs": report.statistics.verified_pairs
        }
    }))
}

/// Conventional route-wiring files whose near-identical wrappers are
/// intentional, not refactoring targets.
fn is_boilerplate(path: &str) -> bool {
    let lower = path.to_ascii_lowercase();
    let file = lower.rsplit(['/', '\\']).next().unwrap_or(&lower);
    [".router.", ".routes.", ".handlers."]
        .iter()
        .any(|marker| file.contains(marker))
}

/// Whether any line of the fragment carries executable control flow, as
/// opposed to an immutable declarative catalog of data.
fn has_control_flow(
    root: &std::path::Path,
    path: &str,
    start_line: u32,
    end_line: u32,
    sources: &mut std::collections::HashMap<String, Vec<String>>,
) -> bool {
    const MARKERS: &[&str] = &[
        "if ", "if(", "for ", "for(", "while ", "while(", "return", "=>", "function", "throw",
        "await ", "switch", "yield", "match ", "loop ", "?.",
    ];
    let lines = sources.entry(path.to_owned()).or_insert_with(|| {
        std::fs::read_to_string(root.join(path))
            .map(|text| text.lines().map(str::to_owned).collect())
            .unwrap_or_default()
    });
    let start = usize::try_from(start_line.saturating_sub(1)).unwrap_or(0);
    let end = usize::try_from(end_line).unwrap_or(0).min(lines.len());
    if start >= end {
        // Unreadable fragments stay visible rather than silently vanishing.
        return true;
    }
    lines[start..end]
        .iter()
        .any(|line| MARKERS.iter().any(|marker| line.contains(marker)))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PathClass {
    Product,
    Test,
    Classified,
}

/// Classifies a repository path the way review tools should treat its
/// evidence: production, test, or otherwise non-product.
fn path_class(path: &str) -> PathClass {
    let lower = path.to_ascii_lowercase();
    let segments = lower.split(['/', '\\']).collect::<Vec<_>>();
    let has = |names: &[&str]| segments.iter().any(|segment| names.contains(segment));
    if has(&[
        "__test__",
        "__tests__",
        "test",
        "tests",
        "e2e",
        "spec",
        "specs",
    ]) {
        return PathClass::Test;
    }
    let file = segments.last().copied().unwrap_or_default();
    if [
        ".test.", ".tests.", ".spec.", ".itest.", ".e2e.", "_test.", "_spec.",
    ]
    .iter()
    .any(|marker| file.contains(marker))
    {
        return PathClass::Test;
    }
    if has(&[
        "generated",
        "vendor",
        "vendored",
        "mock",
        "mocks",
        "fixture",
        "fixtures",
        "stories",
        "docs",
        "benchmark",
        "benchmarks",
        "temp",
        "dist",
        "build",
    ]) || file.contains(".min.")
        || file.contains(".openapi.")
    {
        return PathClass::Classified;
    }
    PathClass::Product
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
