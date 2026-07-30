use super::model::VerificationChecks;
use crate::engine::RepositoryState;
use crate::operations::{optional_bool, optional_str};
use blazingly_json::{Value, json};

pub(super) fn build_checks(
    state: &RepositoryState,
    phase: &str,
    base_ref: &str,
    task: &str,
    files: &[String],
    args: &Value,
) -> Result<VerificationChecks, String> {
    Ok(VerificationChecks {
        graph_baseline: graph_baseline(state, phase, base_ref, args)?,
        architecture: architecture(state, phase, task, files)?,
        audit: audit(state, phase, base_ref)?,
        duplicates: duplicates(state, phase, args)?,
        api_contract: api_contract(state, args)?,
    })
}

fn graph_baseline(
    state: &RepositoryState,
    phase: &str,
    base_ref: &str,
    args: &Value,
) -> Result<Value, String> {
    if phase != "verify" {
        return Ok(json!({"state": "PLANNED", "baseline": base_ref}));
    }
    let graph_args = if let Some(head_ref) = optional_str(args, "head_ref")? {
        json!({"base_ref": base_ref, "head_ref": head_ref, "max_results": 100})
    } else {
        json!({"base_ref": base_ref, "max_results": 100})
    };
    Ok(json!({
        "state": "PASS",
        "evidence": crate::operations::history::graph_diff(state, &graph_args)?
    }))
}

fn audit(state: &RepositoryState, phase: &str, base_ref: &str) -> Result<Value, String> {
    if phase == "verify" {
        crate::operations::health::audit(
            state,
            &json!({"max_findings": 50, "base_ref": base_ref, "debt": "new"}),
        )
    } else {
        Ok(json!({"status": "PLANNED", "baseline": base_ref}))
    }
}

fn architecture(
    state: &RepositoryState,
    phase: &str,
    task: &str,
    files: &[String],
) -> Result<Value, String> {
    if files.is_empty() {
        Ok(json!({"state": "NOT_APPLICABLE", "reason": "no changed files"}))
    } else if phase == "verify" {
        crate::operations::architecture::verify(state)
    } else {
        Ok(json!({
            "state": "PLANNED",
            "evidence": crate::operations::architecture::prepare(
                state,
                &json!({"intent": task, "files": files})
            )?
        }))
    }
}

fn duplicates(state: &RepositoryState, phase: &str, args: &Value) -> Result<Value, String> {
    if !optional_bool(args, "duplicate_ratchet")?.unwrap_or(true) {
        return Ok(json!({"state": "SKIPPED", "enabled": false}));
    }
    if phase != "verify" {
        return Ok(json!({"state": "PLANNED", "enabled": true}));
    }
    let report = crate::operations::health::duplicates(
        state,
        &json!({"mode": "renamed", "top_n": 50, "min_tokens": 50}),
    )?;
    let families = report["families"].as_array().map_or(0, Vec::len);
    Ok(json!({
        "state": if families == 0 {"PASS"} else {"REVIEW"},
        "enabled": true,
        "reason": if families == 0 {
            Value::Null
        } else {
            json!("clone families exist; compare them with the immutable baseline before accepting the change")
        },
        "report": report
    }))
}

fn api_contract(state: &RepositoryState, args: &Value) -> Result<Value, String> {
    let Some(contract) = args.get("api_contract") else {
        return Ok(json!({
            "state": "SKIPPED",
            "reason": "no api_contract scope was requested"
        }));
    };
    let evidence = super::super::trace_api(state, contract)?;
    let state = if evidence
        .pointer("/verdict/code")
        .and_then(Value::as_str)
        .is_some_and(|code| {
            matches!(
                code,
                "HTTP_METHOD_MISMATCH" | "EVENT_CONTRACT_MISMATCH" | "TYPED_API_CONTRACT_MISMATCH"
            )
        }) {
        "REVIEW"
    } else {
        "PASS"
    };
    Ok(json!({"state": state, "evidence": evidence}))
}
