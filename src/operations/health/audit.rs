use super::{coverage, cycles, debt, dependencies, runtime, test_evidence};
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
    let focused = category.is_some_and(|value| value != "all");
    let dependency_report = if category_matches(category, "dependencies") {
        dependencies::report(state, max, (true, min_severity))
    } else {
        skipped_section()
    };
    let runtime_report = if category_matches(category, "runtime") {
        runtime::runtime(state, max, true, min_severity)
    } else {
        skipped_section()
    };
    let coverage_report = if focused {
        skipped_section()
    } else {
        coverage::coverage(state, &json!({}))?
    };
    let tests_enabled = optional_bool(args, "include_tests")?.unwrap_or(true)
        && category_matches(category, "tests");
    let test_report = test_evidence::report(state, args, tests_enabled, max)?;
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
    let (resolution, thin) = edge_resolution(state);
    let reviewing = [&runtime_report, &dependency_report]
        .iter()
        .any(|report| report["status"] == "REVIEW")
        || has_cycles
        || !findings.is_empty()
        || test_report["status"] == "FAIL"
        || thin;
    let debt = debt::debt(state, args, max, &runtime_report)?;
    let mut report = json!({
        "status": if reviewing {"REVIEW"} else {"PASS"},
        "execution": {"status": "COMPLETE"},
        "resolution": resolution,
        "findings": findings,
        "cycles": cycles,
        "languages": language_counts,
        "dependency_report": dependency_report,
        "runtime_report": runtime_report,
        "coverage_report": coverage_report,
        "test_report": test_report,
        "debt": debt
    });
    // The matrix is a static property of the build. Callers that need it opt in.
    if optional_bool(args, "include_capabilities")?.unwrap_or(false)
        && let Some(object) = report.as_object_mut()
    {
        object.insert(
            "capability_matrix".to_owned(),
            json!(state.snapshot().capabilities),
        );
    }
    Ok(report)
}

fn validate_options(args: &Value) -> Result<(Option<&str>, u8), String> {
    let _ = optional_bool(args, "include_tests")?;
    let _ = optional_bool(args, "include_classified")?;
    let category = optional_str(args, "category")?;
    if category.is_some_and(|value| {
        !matches!(
            value,
            "all" | "diagnostics" | "structure" | "dependencies" | "runtime" | "tests"
        )
    }) {
        return Err(
            "category must be all, diagnostics, structure, dependencies, runtime, or tests"
                .to_owned(),
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

fn skipped_section() -> Value {
    json!({
        "status": "SKIPPED",
        "collected": false,
        "findings_total": 0,
        "findings": [],
        "execution": {"status": "COMPLETE"},
        "reason": "this section was not requested"
    })
}

fn edge_resolution(state: &RepositoryState) -> (Value, bool) {
    let evidence = &state.census().evidence;
    let parsed = evidence.get("parsed").copied().unwrap_or(0);
    let resolved = evidence.get("resolved").copied().unwrap_or(0);
    let thin = parsed >= 200 && resolved.saturating_mul(5) < parsed;
    (
        json!({
            "parsed_edges": parsed,
            "resolved_edges": resolved,
            "thin": thin
        }),
        thin,
    )
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
