use super::{coverage, cycles, debt, dependencies, runtime};
use crate::engine::RepositoryState;
use crate::operations::{optional_bool, optional_str, optional_u64};
use blazingly_json::{Value, json};
use std::collections::BTreeMap;

pub(in crate::operations) fn audit(state: &RepositoryState, args: &Value) -> Result<Value, String> {
    let max = usize::try_from(optional_u64(args, "max_findings")?.unwrap_or(30))
        .map_err(|_| "max_findings is too large".to_owned())?;
    let (category, min_severity) = validate_options(args)?;
    let all_cycles =
        if category_matches(category, "structure") && severity_at_least("medium", min_severity) {
            cycles::runtime_dependency_cycles(state.graph(), args)
        } else {
            Vec::new()
        };
    let has_cycles = !all_cycles.is_empty();
    let cycles = all_cycles.into_iter().take(max).collect::<Vec<_>>();
    let mut language_counts = BTreeMap::<String, u64>::new();
    for node in state.graph().nodes() {
        if let Some(language) = &node.language {
            *language_counts.entry(language.clone()).or_default() += 1;
        }
    }
    let dependency_report = dependencies::report(
        state,
        max,
        (category_matches(category, "dependencies"), min_severity),
    );
    let runtime_report = runtime::runtime(
        state,
        max,
        category_matches(category, "runtime"),
        min_severity,
    );
    let coverage_report = coverage::coverage(state, &json!({}))?;
    let findings = if category_matches(category, "diagnostics") {
        state
            .snapshot()
            .diagnostics
            .iter()
            .take(max)
            .collect::<Vec<_>>()
    } else {
        Vec::new()
    };
    let reviewing = [&runtime_report, &dependency_report]
        .iter()
        .any(|report| report["status"] == "REVIEW")
        || has_cycles
        || !findings.is_empty();
    let debt = debt::debt(state, args, max, &runtime_report)?;
    Ok(json!({
        "status": if reviewing {"REVIEW"} else {"PASS"},
        "execution": {"status": "COMPLETE"},
        "findings": findings,
        "cycles": cycles,
        "cycle_model": {
            "scope": "production file-level runtime dependencies",
            "relations": ["runtime imports", "cyclic cross-file call chains", "mounts", "transport producer-to-consumer"],
            "excluded": ["containment", "symbol ownership", "references", "inheritance", "implements", "re-exports", "type-only and compile-time imports", "test and classified files by default"],
        },
        "languages": language_counts,
        "capability_matrix": state.snapshot().capabilities,
        "dependency_report": dependency_report,
        "runtime_report": runtime_report,
        "coverage_report": coverage_report,
        "evidence": {
            "structure": {
                "present": true,
                "scope": "registered lossless and structural language adapters with typed graph provenance"
            },
            "dependencies": dependency_report["manifest_evidence"].clone(),
            "runtime": runtime_report["runtime_evidence"].clone(),
            "coverage": coverage_report["measured_coverage"].clone()
        },
        "debt": debt
    }))
}

fn validate_options(args: &Value) -> Result<(Option<&str>, u8), String> {
    let _ = optional_bool(args, "include_tests")?;
    let _ = optional_bool(args, "include_classified")?;
    let category = optional_str(args, "category")?;
    if category.is_some_and(|value| {
        !matches!(
            value,
            "all" | "diagnostics" | "structure" | "dependencies" | "runtime"
        )
    }) {
        return Err(
            "category must be all, diagnostics, structure, dependencies, or runtime".to_owned(),
        );
    }
    let min_severity = match optional_str(args, "min_severity")?.unwrap_or("low") {
        "low" => 0,
        "medium" => 1,
        "high" => 2,
        "critical" => 3,
        _ => return Err("min_severity must be low, medium, high, or critical".to_owned()),
    };
    if let Some(view) = optional_str(args, "debt")?
        && !matches!(view, "new" | "existing" | "all")
    {
        return Err("debt must be new, existing, or all".to_owned());
    }
    if let Some(changed) = args.get("changed_files") {
        let changed = changed
            .as_array()
            .ok_or_else(|| "changed_files must be an array of strings".to_owned())?;
        if changed.iter().any(|path| path.as_str().is_none()) {
            return Err("changed_files must contain only strings".to_owned());
        }
    }
    Ok((category, min_severity))
}

fn category_matches(selected: Option<&str>, candidate: &str) -> bool {
    selected.is_none_or(|value| value == "all" || value == candidate)
}

pub(super) fn severity_at_least(severity: &str, minimum: u8) -> bool {
    let rank = match severity {
        "low" => 0,
        "medium" => 1,
        "high" => 2,
        "critical" => 3,
        _ => return false,
    };
    rank >= minimum
}
