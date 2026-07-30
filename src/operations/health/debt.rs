use crate::engine::RepositoryState;
use crate::operations::optional_str;
use blazingly_json::{Value, json};
#[cfg(feature = "git")]
use {
    super::cycles::runtime_dependency_cycles,
    super::paths::is_non_product,
    super::runtime::{product_sources, runtime_findings},
};

/// Baseline comparison needs Git object reads, which the minimal build omits.
#[cfg(not(feature = "git"))]
pub(super) fn debt(
    _state: &RepositoryState,
    args: &Value,
    _max: usize,
    _runtime_report: &Value,
) -> Result<Value, String> {
    let requested = optional_str(args, "base_ref")?;
    Ok(json!({
        "status": "COMPLETE",
        "comparison": {
            "present": false,
            "reason": if requested.is_some() {
                "baseline comparison requires the Git-enabled build"
            } else {
                "no base_ref was requested"
            }
        }
    }))
}

/// Separates new findings from debt inherited from an immutable Git baseline.
#[cfg(feature = "git")]
pub(super) fn debt(
    state: &RepositoryState,
    args: &Value,
    max: usize,
    runtime_report: &Value,
) -> Result<Value, String> {
    const DEBT_CAP: usize = 5_000;

    let Some(base_ref) = optional_str(args, "base_ref")? else {
        return Ok(json!({
            "status": "COMPLETE",
            "comparison": {
                "present": false,
                "reason": "no base_ref was requested"
            }
        }));
    };
    let view = optional_str(args, "debt")?.unwrap_or("new");
    if !matches!(view, "new" | "existing" | "all") {
        return Err("debt must be new, existing, or all".to_owned());
    }
    let (baseline_graph, baseline_sources) =
        crate::operations::history::revision_evidence(state, base_ref)?;
    let baseline_sources = baseline_sources
        .into_iter()
        .filter(|(path, _, _)| !is_non_product(path));
    let (baseline_runtime, _, _) = runtime_findings(baseline_sources, DEBT_CAP);
    let mut baseline_ids = baseline_runtime
        .iter()
        .filter_map(|finding| finding["id"].as_str().map(str::to_owned))
        .collect::<std::collections::BTreeSet<_>>();
    baseline_ids.extend(
        runtime_dependency_cycles(&baseline_graph, args)
            .iter()
            .map(|component| cycle_id(component)),
    );

    let (mut current, _, truncated) = runtime_findings(product_sources(state), DEBT_CAP);
    let _ = runtime_report;
    for component in runtime_dependency_cycles(state.graph(), args) {
        current.push(json!({
            "id": cycle_id(&component),
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
    Ok(json!({
        "status": "COMPLETE",
        "comparison": {"present": true},
        "base_ref": base_ref,
        "baseline_nodes": baseline_graph.nodes().len(),
        "truncated": truncated,
        "view": view,
        "comparable_categories": ["runtime", "structure"],
        "uncomparable_categories": {
            "dependencies": "manifests and lockfiles are read from the worktree, not the baseline checkout",
            "coverage": "measured coverage reports are not stored in Git revisions"
        },
        "counts": {"new": new.len(), "existing": existing.len(), "fixed": fixed.len()},
        "findings": if view == "all" {
            json!({"new": new, "existing": existing, "fixed": fixed})
        } else {
            json!(selected.iter().take(max).collect::<Vec<_>>())
        },
    }))
}

#[cfg(feature = "git")]
fn cycle_id(component: &[String]) -> String {
    format!(
        "structure.cycle:{}",
        fingerprint(component.iter().map(String::as_str))
    )
}

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
