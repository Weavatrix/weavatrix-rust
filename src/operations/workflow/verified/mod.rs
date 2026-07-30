//! Evidence-led verified-change orchestration.

mod checks;
mod context;
mod model;
mod test_plan;
mod verdict;

use crate::engine::RepositoryState;
use crate::operations::{arg_str, optional_str};
use blazingly_json::{Value, json};
use checks::build_checks;
use context::build_context;
use test_plan::build_test_evidence;
use verdict::assess;

pub(in crate::operations) fn verified_change(
    state: &RepositoryState,
    args: &Value,
) -> Result<Value, String> {
    let task = arg_str(args, "task")?;
    let phase = optional_str(args, "phase")?.unwrap_or("plan");
    if !matches!(phase, "plan" | "verify") {
        return Err("phase must be plan or verify".to_owned());
    }
    let base_ref = optional_str(args, "base_ref")?.unwrap_or("HEAD");
    let impact = super::change::change_impact(state, args)?;
    let files = impact["changed_files"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .map(str::to_owned)
        .collect::<Vec<_>>();
    let context = build_context(state, phase, task, &files, &impact, args)?;
    let checks = build_checks(state, phase, base_ref, task, &files, args)?;
    let tests = build_test_evidence(&impact, args)?;
    let assessment = assess(phase, &files, &checks, &tests);

    Ok(json!({
        "schemaVersion": "weavatrix.verified-change.v1",
        "status": "COMPLETE",
        "task": task,
        "phase": phase,
        "verdict": assessment.verdict,
        "blockers": assessment.blockers,
        "impact": impact,
        "changeImpact": impact,
        "retrieval": context.retrieval,
        "editContexts": context.edit_contexts,
        "dataFlow": context.data_flow,
        "graphBaseline": checks.graph_baseline,
        "architecture": checks.architecture,
        "audit": checks.audit,
        "duplicates": checks.duplicates,
        "apiContract": checks.api_contract,
        "tests": tests.value,
        "limitations": assessment.limitations,
        "source_mutation": "NONE",
        "test_execution": tests.value["execution"].clone()
    }))
}
