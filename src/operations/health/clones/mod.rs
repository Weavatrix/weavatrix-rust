//! Clone-family filtering and release-facing evidence.

use crate::engine::RepositoryState;
use blazingly_json::Value;
#[cfg(feature = "clone")]
use {
    super::paths::{PathClass, path_class},
    super::runtime::rust_cfg_test_lines,
    crate::operations::{optional_bool, optional_str, optional_u64},
    std::collections::{BTreeSet, HashMap},
    weavatrix_clone::{
        CloneConfig, CloneDetector, CloneFamily, ClonePair, CloneReport, DetectionMode,
        RepositoryCloneDetector, Similarity,
    },
};

#[cfg(feature = "clone")]
mod families;
#[cfg(feature = "clone")]
mod render;
#[cfg(feature = "clone")]
mod threshold;

#[cfg(feature = "clone")]
pub(in crate::operations) fn duplicates(
    state: &RepositoryState,
    args: &Value,
) -> Result<Value, String> {
    let report = RepositoryCloneDetector::new(clone_detector(args)?)
        .detect(state.root())
        .map_err(|error| error.to_string())?;
    let CloneReport {
        families: raw_families,
        pairs: raw_pairs,
        statistics,
    } = report;
    let raw_family_count = raw_families.len();
    let top = usize::try_from(optional_u64(args, "top_n")?.unwrap_or(15))
        .map_err(|_| "top_n is too large".to_owned())?;
    let visibility = Visibility {
        include_tests: optional_bool(args, "include_tests")?.unwrap_or(false),
        include_classified: optional_bool(args, "include_classified")?.unwrap_or(false),
    };
    let mut test_lines = HashMap::<String, BTreeSet<usize>>::new();
    let mut visible = |path: &str, start: u32, end: u32| {
        clone_location_visible(state, path, start, end, visibility, &mut test_lines)
    };
    let (mut pairs, suppressed_pairs) = visible_pairs(raw_pairs, &mut visible);
    let families = families::rebuild(&pairs);
    let suppressed_families = raw_family_count.saturating_sub(families.len());
    let (families, low_signal) = suppress_low_signal(state, args, families)?;
    let visible_pair_ids = families
        .iter()
        .flat_map(|family| family.pair_ids.iter().cloned())
        .collect::<BTreeSet<_>>();
    pairs.retain(|pair| visible_pair_ids.contains(&pair.id));
    let (families, pairs) = families::limit(families, pairs, top);
    Ok(render::report(
        &statistics,
        &families,
        &pairs,
        top,
        suppressed_families,
        suppressed_pairs,
        low_signal,
    ))
}

#[cfg(feature = "clone")]
fn clone_detector(args: &Value) -> Result<CloneDetector, String> {
    let mode = match optional_str(args, "mode")?.unwrap_or("near_miss") {
        "strict" | "exact" => DetectionMode::Exact,
        "renamed" => DetectionMode::Renamed,
        "near_miss" => DetectionMode::NearMiss,
        other => {
            return Err(format!(
                "mode must be strict, exact, renamed, or near_miss; got {other}"
            ));
        }
    };
    let min_tokens = usize::try_from(optional_u64(args, "min_tokens")?.unwrap_or(50))
        .map_err(|_| "min_tokens is too large")?;
    CloneDetector::new(CloneConfig {
        mode,
        min_tokens,
        min_similarity: Similarity::from_permille(threshold::similarity_permille(args)?),
        ..CloneConfig::default()
    })
    .map_err(|error| error.to_string())
}

#[cfg(feature = "clone")]
fn visible_pairs(
    pairs: Vec<ClonePair>,
    visible: &mut impl FnMut(&str, u32, u32) -> bool,
) -> (Vec<ClonePair>, usize) {
    let before = pairs.len();
    let mut pairs = pairs
        .into_iter()
        .filter(|pair| {
            visible(
                &pair.left.path,
                pair.left.span.start_line,
                pair.left.span.end_line,
            ) && visible(
                &pair.right.path,
                pair.right.span.start_line,
                pair.right.span.end_line,
            )
        })
        .collect::<Vec<_>>();
    let suppressed = before.saturating_sub(pairs.len());
    pairs.sort_by_key(|pair| core::cmp::Reverse(pair.evidence.compared_tokens));
    (pairs, suppressed)
}

#[cfg(feature = "clone")]
#[derive(Clone, Copy)]
struct Visibility {
    include_tests: bool,
    include_classified: bool,
}

#[cfg(feature = "clone")]
#[derive(Clone, Copy)]
struct LowSignalSuppression {
    pub(super) boilerplate: usize,
    pub(super) declarative: usize,
}

#[cfg(feature = "clone")]
fn suppress_low_signal(
    state: &RepositoryState,
    args: &Value,
    families: Vec<CloneFamily>,
) -> Result<(Vec<CloneFamily>, LowSignalSuppression), String> {
    let include_boilerplate = optional_bool(args, "include_boilerplate")?.unwrap_or(false);
    let include_declarative = optional_bool(args, "include_declarative")?.unwrap_or(false);
    let mut sources = HashMap::<String, Vec<String>>::new();
    let mut suppression = LowSignalSuppression {
        boilerplate: 0,
        declarative: 0,
    };
    let families = families
        .into_iter()
        .filter(|family| {
            if !include_boilerplate
                && family
                    .members
                    .iter()
                    .all(|member| is_boilerplate(&member.path))
            {
                suppression.boilerplate += 1;
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
                suppression.declarative += 1;
                return false;
            }
            true
        })
        .collect();
    Ok((families, suppression))
}

#[cfg(feature = "clone")]
fn clone_location_visible(
    state: &RepositoryState,
    path: &str,
    start: u32,
    end: u32,
    visibility: Visibility,
    test_lines: &mut HashMap<String, BTreeSet<usize>>,
) -> bool {
    let mut class = path_class(path);
    if class == PathClass::Product
        && std::path::Path::new(path)
            .extension()
            .is_some_and(|extension| extension.eq_ignore_ascii_case("rs"))
    {
        let lines = test_lines.entry(path.to_owned()).or_insert_with(|| {
            std::fs::read_to_string(state.root().join(path))
                .map_or_else(|_| BTreeSet::new(), |source| rust_cfg_test_lines(&source))
        });
        if (start..=end).all(|line| lines.contains(&(line as usize))) {
            class = PathClass::Test;
        }
    }
    match class {
        PathClass::Product => true,
        PathClass::Test => visibility.include_tests,
        PathClass::Classified => visibility.include_classified,
    }
}

#[cfg(feature = "clone")]
fn is_boilerplate(path: &str) -> bool {
    let lower = path.to_ascii_lowercase();
    let file = lower.rsplit(['/', '\\']).next().unwrap_or(&lower);
    [".router.", ".routes.", ".handlers."]
        .iter()
        .any(|marker| file.contains(marker))
}

#[cfg(feature = "clone")]
fn has_control_flow(
    root: &std::path::Path,
    path: &str,
    start_line: u32,
    end_line: u32,
    sources: &mut HashMap<String, Vec<String>>,
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
        return true;
    }
    lines[start..end]
        .iter()
        .any(|line| MARKERS.iter().any(|marker| line.contains(marker)))
}

#[cfg(not(feature = "clone"))]
pub(in crate::operations) fn duplicates(
    _state: &RepositoryState,
    _args: &Value,
) -> Result<Value, String> {
    Err("clone capability is not compiled".to_owned())
}
