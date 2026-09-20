mod local;
mod output;
mod recognize;
mod scenario;

use super::action;
use super::workflow::{self, Job, Step, Workflow};
use crate::engine::RepositoryState;
use crate::operations::{arg_str, optional_str, optional_u64};
use blazingly_json::{Value, json};
use output::{
    condition_class, matrix_summary, permissions, public_uses, step_summary, working_directory,
};
use scenario::Scenario;
use std::collections::BTreeMap;

pub(super) fn report(state: &RepositoryState, args: &Value) -> Result<Value, String> {
    crate::operations::reject_unknown_arguments(
        "ci_restrictions",
        args,
        &["scope", "scenario", "max_results", "token_budget"],
    )?;
    let scenario = Scenario::parse(args)?;
    let scope = optional_str(args, "scope")?;
    let max = optional_u64(args, "max_results")?.unwrap_or(100).min(500) as usize;
    let collection = workflow::collect(state);
    let mut restrictions = Vec::new();
    let mut workflows = Vec::new();
    let mut unresolved = collection.unresolved;
    for workflow in collection
        .workflows
        .iter()
        .filter(|workflow| scope.is_none_or(|scope| workflow.path.contains(scope)))
    {
        workflows.push(project_workflow(
            state,
            workflow,
            &collection.actions,
            &collection.workflows,
            &scenario,
            &mut restrictions,
            &mut unresolved,
        ));
    }
    let total = restrictions.len();
    restrictions.truncate(max);
    let mut result = json!({
        "status": if unresolved.is_empty() {"COMPLETE"} else {"INCOMPLETE"},
        "model": "local declared workflow and literal checker evidence",
        "workflows": workflows,
        "restrictions": restrictions,
        "restrictions_total": total,
        "truncated": total > max,
        "unresolved": unresolved,
        "remote_enforcement": "NOT_OBSERVED",
        "execution": "NOT_OBSERVED"
    });
    super::attach_inventory(state, &collection.inventory, &mut result);
    let budget = crate::operations::token_budget::requested(args)?;
    crate::operations::token_budget::fit(
        &mut result,
        budget,
        &[
            "/workflows",
            "/restrictions",
            "/unresolved",
            "/config_inputs/read",
        ],
    );
    Ok(result)
}

fn project_workflow(
    state: &RepositoryState,
    workflow: &Workflow,
    actions: &[action::Action],
    all_workflows: &[Workflow],
    scenario: &Scenario,
    restrictions: &mut Vec<Value>,
    unresolved: &mut Vec<String>,
) -> Value {
    let mut jobs = Vec::new();
    for job in &workflow.jobs {
        let mut steps = Vec::new();
        for step in &job.steps {
            let id = format!("{}/{}/{}", workflow.path, job.id, step.index);
            let working = step
                .working_directory
                .as_deref()
                .or(job.working_directory.as_deref())
                .or(workflow.working_directory.as_deref());
            let directory = working_directory(state.root(), working);
            let invocations = recognize::in_step(directory.as_deref(), step);
            let mut counts = BTreeMap::<&str, usize>::new();
            for found in invocations {
                let sequence = counts.entry(found.kind).or_default();
                *sequence += 1;
                let effect = failure_effect_for(job, step, &found);
                let span = crate::model::evidence::raw_byte_span(
                    &workflow.bytes,
                    found.source_text.as_bytes(),
                );
                restrictions.push(json!({
                    "id": format!("{id}/{}/{}", found.kind, sequence),
                    "kind": found.kind,
                    "declared": true,
                    "detail": found.detail,
                    "target": found.target,
                    "target_resolved": found.target_exists,
                    "recognition": found.reliability,
                    "source": {"path": workflow.path, "content_digest": workflow.digest,
                               "job": job.id, "step": step.index, "byte_span": span},
                    "applicability": applicability(workflow, job, step, scenario),
                    "execution": "NOT_OBSERVED",
                    "failure_effect": effect,
                    "remote_enforcement": "NOT_OBSERVED"
                }));
            }
            local::inspect_script(&mut local::ScriptInvocation {
                state,
                workflow,
                job,
                step,
                working,
                id: &id,
                restrictions,
                unresolved,
            });
            if let Some(uses) = &step.uses {
                if uses.starts_with("./") {
                    local::inspect_action(&mut local::ActionInvocation {
                        state,
                        actions,
                        workflow,
                        job,
                        step,
                        id: &id,
                        scenario,
                        restrictions,
                        unresolved,
                    });
                } else {
                    unresolved.push(format!("{id}: remote action body not available"));
                }
            }
            steps.push(step_summary(step, working));
        }
        let reusable_resolved = resolve_reusable(workflow, job, all_workflows, unresolved);
        let matrix = summarize_matrix(workflow, job, unresolved);
        jobs.push(json!({
            "id": job.id,
            "needs": job.needs,
            "outputs": job.outputs,
            "runs_on": public_uses(job.runs_on.as_deref()),
            "working_directory": public_uses(job.working_directory.as_deref().or(workflow.working_directory.as_deref())),
            "matrix": matrix,
            "condition": condition_class(job.condition.as_deref()),
            "continue_on_error": condition_class(job.continue_on_error.as_deref()),
            "reusable": public_uses(job.reusable.as_deref()),
            "reusable_resolved": reusable_resolved,
            "steps": steps
        }));
    }
    json!({
        "path": workflow.path,
        "content_digest": workflow.digest,
        "triggers": workflow.triggers,
        "permissions": permissions(workflow.permissions.as_ref()),
        "jobs": jobs
    })
}

fn summarize_matrix(workflow: &Workflow, job: &Job, unresolved: &mut Vec<String>) -> Value {
    let matrix = matrix_summary(job.matrix.as_ref());
    if matrix["status"] == "INCOMPLETE" {
        unresolved.push(format!("{}/{}: dynamic matrix", workflow.path, job.id));
    }
    matrix
}

fn resolve_reusable(
    workflow: &Workflow,
    job: &Job,
    all_workflows: &[Workflow],
    unresolved: &mut Vec<String>,
) -> bool {
    let resolved = job.reusable.as_deref().is_some_and(|reference| {
        reference
            .strip_prefix("./")
            .is_some_and(|path| all_workflows.iter().any(|workflow| workflow.path == path))
    });
    if job.reusable.is_some() && !resolved {
        unresolved.push(format!(
            "{}/{}: reusable workflow body unavailable",
            workflow.path, job.id
        ));
    }
    resolved
}

pub(super) fn explain(state: &RepositoryState, args: &Value) -> Result<Value, String> {
    crate::operations::reject_unknown_arguments("explain_restriction", args, &["id", "scenario"])?;
    let id = arg_str(args, "id")?;
    let report = report(
        state,
        &json!({"max_results": 500, "scenario": args.get("scenario")}),
    )?;
    let restriction = report["restrictions"]
        .as_array()
        .into_iter()
        .flatten()
        .find(|item| item["id"] == id);
    Ok(if let Some(item) = restriction {
        json!({
            "status": "FOUND", "restriction": item,
            "analysis_identity": report["analysis_identity"],
            "limitations": ["local workflow presence is not execution or remote merge enforcement"]
        })
    } else {
        json!({
            "status": if report["restrictions_total"].as_u64().unwrap_or(0) > 500 {
                "INCOMPLETE"
            } else { "NOT_FOUND" },
            "id": id
        })
    })
}

fn applicability(workflow: &Workflow, job: &Job, step: &Step, scenario: &Scenario) -> &'static str {
    let trigger = scenario::trigger(workflow, scenario);
    if trigger != "TRIGGERED" {
        return trigger;
    }
    if matches!(condition_class(job.condition.as_deref()), "FALSE")
        || matches!(condition_class(step.condition.as_deref()), "FALSE")
    {
        return "SKIPPED";
    }
    if condition_class(job.condition.as_deref()) == "EXPRESSION"
        || condition_class(step.condition.as_deref()) == "EXPRESSION"
    {
        "UNDETERMINED"
    } else if !job.needs.is_empty() {
        "UPSTREAM_UNKNOWN"
    } else {
        "APPLIES_LOCALLY"
    }
}

fn failure_effect(job: &Job, step: &Step) -> &'static str {
    match (
        job.continue_on_error.as_deref(),
        step.continue_on_error.as_deref(),
    ) {
        (Some("true"), _) | (_, Some("true")) => "SOFTENED",
        (Some(value), _) | (_, Some(value)) if value != "false" => "UNDETERMINED",
        _ => "PROPAGATES_IN_JOB",
    }
}

fn failure_effect_for(job: &Job, step: &Step, found: &recognize::Found) -> &'static str {
    if found.source_text.contains("|| true") {
        "SOFTENED"
    } else if found.source_text.starts_with("if ") || found.reliability == "KNOWN_COUNTEREXAMPLE" {
        "UNDETERMINED"
    } else {
        failure_effect(job, step)
    }
}
