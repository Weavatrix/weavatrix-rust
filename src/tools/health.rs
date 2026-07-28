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
#[cfg(feature = "clone")]
fn is_boilerplate(path: &str) -> bool {
    let lower = path.to_ascii_lowercase();
    let file = lower.rsplit(['/', '\\']).next().unwrap_or(&lower);
    [".router.", ".routes.", ".handlers."]
        .iter()
        .any(|marker| file.contains(marker))
}

/// Whether any line of the fragment carries executable control flow, as
/// opposed to an immutable declarative catalog of data.
#[cfg(feature = "clone")]
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
        .filter(|(slot, _)| crate::tools::node_is_visible(state, *slot, args))
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
    let dependency_report = super::health_dependencies::report(state, max);
    let runtime_report = super::health_runtime::runtime(state, max);
    let advisory_report = super::health_runtime::advisories(state, max);
    let malware_requested = args
        .get("include_malware_scan")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let malware_report = super::health_runtime::malware(state, max, malware_requested);
    let coverage_report = super::health_coverage::coverage(state);
    let reviewing = [
        &runtime_report,
        &advisory_report,
        &malware_report,
        &dependency_report,
    ]
    .iter()
    .any(|report| report["status"] == "REVIEW");
    json!({
        "status": if state.snapshot().diagnostics.is_empty() && !reviewing {"PASS"} else {"REVIEW"},
        "findings": state.snapshot().diagnostics.iter().take(max).collect::<Vec<_>>(),
        "cycles": cycles,
        "languages": language_counts,
        "capability_matrix": state.snapshot().capabilities,
        "dependency_report": dependency_report,
        "runtime_report": runtime_report,
        "advisory_report": advisory_report,
        "malware_report": malware_report,
        "coverage_report": coverage_report,
        "completeness": {
            "structure": "PARTIAL_LANGUAGE_AWARE",
            "dependencies": "PARTIAL_MANIFEST_AWARE",
            "runtime": runtime_report["completeness"].clone(),
            "advisories": advisory_report["completeness"].clone(),
            "malware": malware_report["completeness"].clone(),
            "coverage": coverage_report["actualCoverage"].clone()
        },
        "debt": debt(state, args, max, &cycles, &runtime_report)
    })
}

/// Baseline comparison needs Git object reads, which the minimal build omits.
#[cfg(not(feature = "git"))]
fn debt(
    _state: &RepositoryState,
    args: &Value,
    _max: usize,
    _cycles: &[Vec<&str>],
    _runtime_report: &Value,
) -> Value {
    if super::arg_str(args, "base_ref").is_err() {
        return json!({
            "status": "NOT_REQUESTED",
            "message": "pass base_ref (for example HEAD~1 or origin/main) to separate new findings from inherited ones",
        });
    }
    json!({
        "status": "UNAVAILABLE",
        "reason": "baseline comparison reads Git objects; this build was compiled without the git feature",
    })
}

/// Compares deterministic finding identities against an immutable Git
/// baseline so a reviewer can separate new debt from inherited debt.
#[cfg(feature = "git")]
fn debt(
    state: &RepositoryState,
    args: &Value,
    max: usize,
    cycles: &[Vec<&str>],
    runtime_report: &Value,
) -> Value {
    // Both sides use the same generous cap: comparing a truncated current set
    // against a fuller baseline would invent "fixed" findings.
    const DEBT_CAP: usize = 5_000;

    let Ok(base_ref) = super::arg_str(args, "base_ref") else {
        return json!({
            "status": "NOT_REQUESTED",
            "message": "pass base_ref (for example HEAD~1 or origin/main) to separate new findings from inherited ones",
        });
    };
    let view = super::arg_str(args, "debt").unwrap_or("new");
    let (baseline_graph, baseline_sources) =
        match super::history_diff::revision_evidence(state, base_ref) {
            Ok(evidence) => evidence,
            Err(reason) => {
                return json!({
                    "status": "UNAVAILABLE",
                    "base_ref": base_ref,
                    "reason": reason,
                });
            }
        };
    // The baseline must be filtered exactly like the worktree set, or
    // suppressed test evidence would masquerade as fixed debt.
    let baseline_sources = baseline_sources
        .into_iter()
        .filter(|(path, _, _)| !is_non_product(path));
    let (baseline_runtime, _, _) =
        super::health_runtime::runtime_findings(baseline_sources, DEBT_CAP);
    let mut baseline_ids = baseline_runtime
        .iter()
        .filter_map(|finding| finding["id"].as_str().map(str::to_owned))
        .collect::<std::collections::BTreeSet<_>>();
    baseline_ids.extend(
        strongly_connected_components(&baseline_graph)
            .into_iter()
            .filter(|component| component.len() > 1)
            .map(|component| cycle_id(&baseline_graph, &component)),
    );

    let (mut current, _, truncated) = super::health_runtime::runtime_findings(
        super::health_runtime::product_sources(state),
        DEBT_CAP,
    );
    let _ = runtime_report;
    for component in cycles {
        current.push(json!({
            "id": format!("structure.cycle:{}", fingerprint(component.iter().copied())),
            "rule": "structure.dependency_cycle",
            "category": "structure",
            "severity": "medium",
            "members": component,
        }));
    }
    let (new, existing): (Vec<Value>, Vec<Value>) = current.into_iter().partition(|finding| {
        !finding["id"]
            .as_str()
            .is_some_and(|id| baseline_ids.contains(id))
    });
    let current_ids = new
        .iter()
        .chain(existing.iter())
        .filter_map(|finding| finding["id"].as_str())
        .collect::<std::collections::BTreeSet<_>>();
    let fixed = baseline_ids
        .iter()
        .filter(|id| !current_ids.contains(id.as_str()))
        .take(max)
        .collect::<Vec<_>>();
    let selected = match view {
        "existing" => &existing,
        "all" => &Vec::new(),
        _ => &new,
    };
    json!({
        "status": "COMPARED",
        "base_ref": base_ref,
        "baseline_nodes": baseline_graph.nodes().len(),
        "truncated": truncated,
        "view": view,
        "comparable_categories": ["runtime", "structure"],
        "uncomparable_categories": {
            "dependencies": "manifests and lockfiles are read from the worktree, not the baseline checkout",
            "advisories": "supply-chain evidence is not comparable across a source-only baseline",
            "malware": "installed package trees are not part of a Git revision",
            "coverage": "measured coverage reports are not stored in Git revisions"
        },
        "counts": {"new": new.len(), "existing": existing.len(), "fixed": fixed.len()},
        "findings": if view == "all" {
            json!({"new": new, "existing": existing, "fixed": fixed})
        } else {
            json!(selected.iter().take(max).collect::<Vec<_>>())
        },
    })
}

#[cfg(feature = "git")]
fn cycle_id(graph: &weavatrix_graph::Graph, component: &[weavatrix_graph::NodeIndex]) -> String {
    let members = component
        .iter()
        .filter_map(|index| graph.node_at(*index))
        .map(|node| node.id.as_str())
        .collect::<Vec<_>>();
    format!("structure.cycle:{}", fingerprint(members.into_iter()))
}

/// Order-independent fingerprint of a member set.
#[cfg(feature = "git")]
fn fingerprint<'a>(members: impl Iterator<Item = &'a str>) -> String {
    let mut sorted = members.collect::<Vec<_>>();
    sorted.sort_unstable();
    let mut hash = 0xcbf2_9ce4_8422_2325_u64;
    for member in sorted {
        for byte in member.as_bytes() {
            hash ^= u64::from(*byte);
            hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
        }
    }
    format!("{hash:016x}")
}

/// Whether a path's evidence is test or otherwise non-product.
pub(super) fn is_non_product(path: &str) -> bool {
    path_class(path) != PathClass::Product
}

/// Applies the `include_tests` and `include_classified` opt-ins to one path.
pub(super) fn path_is_visible(path: &str, args: &Value) -> bool {
    let opted_in = |key: &str| args.get(key).and_then(Value::as_bool) == Some(true);
    match path_class(path) {
        PathClass::Product => true,
        PathClass::Test => opted_in("include_tests"),
        PathClass::Classified => opted_in("include_classified"),
    }
}

pub fn hot_paths(state: &RepositoryState, args: &Value) -> Value {
    let top = usize::try_from(arg_u64(args, "top_n").unwrap_or(20)).unwrap_or(20);
    let mut ranked = state
        .graph()
        .nodes()
        .iter()
        .enumerate()
        .filter(|(slot, _)| crate::tools::node_is_visible(state, *slot, args))
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
