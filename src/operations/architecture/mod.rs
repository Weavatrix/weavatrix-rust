mod budgets;
mod capabilities;
mod contract;
mod path_pattern;
mod policy_diagnostics;
mod policy_reachability;
mod policy_selectors;
mod policy_templates;
mod rules;
pub(in crate::operations) mod source_metrics;

use crate::engine::RepositoryState;
use crate::operations::arg_str;
use blazingly_json::{Value, json};
use std::collections::BTreeSet;

pub fn contract(state: &RepositoryState, args: &Value) -> Result<Value, String> {
    if arg_str(args, "action").ok() == Some("approve") {
        return Err("read-only MCP never writes architecture contracts".to_owned());
    }
    match contract::load_optional(state)? {
        Some(value) => Ok(json!({
            "state": "CONFIGURED",
            "source": ".weavatrix/architecture.json",
            "contract": value
        })),
        None if arg_str(args, "action").ok() == Some("preview") => Ok(json!({
            "state": "PREVIEW",
            "source": "derived graph folders",
            "contract": contract::starter(state),
            "warning": "no active architecture contract",
            "write": "NONE"
        })),
        None => Ok(contract::not_configured(
            state,
            "Save the starter as .weavatrix/architecture.json, review it, then verify.",
        )),
    }
}

pub fn prepare(state: &RepositoryState, args: &Value) -> Result<Value, String> {
    let files = args
        .get("files")
        .and_then(Value::as_array)
        .ok_or_else(|| "files must be an array".to_owned())?;
    let Some(value) = contract::load_optional(state)? else {
        return Ok(json!({
            "state": "NOT_CONFIGURED",
            "guidance": "PROVISIONAL_STARTER",
            "enforceable": false,
            "intent": args.get("intent"),
            "files": files,
            "starter": contract::starter(state),
            "remediation": contract::remediation(),
            "write": "NONE"
        }));
    };
    let selected = files
        .iter()
        .filter_map(Value::as_str)
        .map(|file| {
            json!({
                "file": file,
                "component": contract::component_for(&value, file),
                "rules": contract::rules_for_file(&value, file)
            })
        })
        .collect::<Vec<_>>();
    Ok(json!({
        "intent": args.get("intent"),
        "files": selected,
        "contract": ".weavatrix/architecture.json"
    }))
}

pub fn verify(state: &RepositoryState) -> Result<Value, String> {
    let Some(value) = contract::load_optional(state)? else {
        return Ok(json!({
            "state": "NOT_CONFIGURED",
            "enforceable": false,
            "new": [],
            "existing": [],
            "excepted": [],
            "fixed": [],
            "starter": contract::starter(state),
            "remediation": contract::remediation(),
            "write": "NONE"
        }));
    };
    rules::validate(&value)?;
    budgets::validate(&value)?;
    let baseline = value
        .pointer("/ratchet/baseline/fingerprints")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .collect::<BTreeSet<_>>();
    let accepted = accepted_exceptions(&value);
    let violations = all_violations(state, &value)?;
    let present = violations
        .iter()
        .filter_map(|item| item["fingerprint"].as_str().map(str::to_owned))
        .collect::<BTreeSet<_>>();
    let (excepted, active): (Vec<_>, Vec<_>) = violations
        .into_iter()
        .partition(|item| accepted.contains(item["fingerprint"].as_str().unwrap_or_default()));
    let (existing, new): (Vec<_>, Vec<_>) = active
        .into_iter()
        .partition(|item| baseline.contains(item["fingerprint"].as_str().unwrap_or_default()));
    // A warn-severity rule reports without blocking; only error rules gate.
    let (warnings, new): (Vec<_>, Vec<_>) = new
        .into_iter()
        .partition(|item| item["rule"]["severity"] == "warn");
    let fixed = baseline
        .iter()
        .filter(|fingerprint| !present.contains(**fingerprint))
        .collect::<Vec<_>>();
    Ok(json!({
        "state": if new.is_empty() {"PASS"} else {"BLOCKED"},
        "enforceable": true,
        "new": new,
        "existing": existing,
        "excepted": excepted,
        "warnings": warnings,
        "fixed": fixed,
        "contract": ".weavatrix/architecture.json"
    }))
}

/// Resolves every declared capability against the endpoints this revision
/// exposes, in both directions: a claim with no evidence behind it, and
/// evidence no claim covers.
pub fn verify_capabilities(state: &RepositoryState, args: &Value) -> Result<Value, String> {
    let limit = crate::operations::optional_u64(args, "max_results")?
        .unwrap_or(capabilities::DEFAULT_MAX_RESULTS);
    let Some(value) = contract::load_optional(state)? else {
        let (starter, total) = capabilities::trimmed(capabilities::starter(state, args), limit);
        return Ok(json!({
            "state": "NOT_CONFIGURED",
            "enforceable": false,
            "served": [],
            "drifted": [],
            "orphaned": [],
            "unmapped": [],
            "unmapped_total": total,
            "starter": {"capabilities": starter, "capabilities_total": total},
            "remediation": contract::remediation(),
            "write": "NONE"
        }));
    };
    let Some(declared) = capabilities::declared(&value) else {
        let (unmapped, total) =
            capabilities::trimmed(capabilities::verify(state, &value, args).unmapped, limit);
        let (starter, _) = capabilities::trimmed(capabilities::starter(state, args), limit);
        return Ok(json!({
            "state": "NOT_DECLARED",
            "enforceable": false,
            "reason": "the architecture contract has no `capabilities` section",
            "served": [],
            "drifted": [],
            "orphaned": [],
            "unmapped": unmapped,
            "unmapped_total": total,
            "starter": {"capabilities": starter, "capabilities_total": total},
            "contract": ".weavatrix/architecture.json",
            "write": "NONE"
        }));
    };
    let declared = declared.len();
    capabilities::validate(&value)?;
    let report = capabilities::verify(state, &value, args);
    let blocked = report.blocked();
    let (unmapped, unmapped_total) = capabilities::trimmed(report.unmapped, limit);
    Ok(json!({
        "state": if blocked {"BLOCKED"} else {"PASS"},
        "enforceable": true,
        "declared": declared,
        "served": report.served,
        "drifted": report.drifted,
        "orphaned": report.orphaned,
        "unmapped": unmapped,
        "unmapped_total": unmapped_total,
        "contract": ".weavatrix/architecture.json"
    }))
}

pub fn explain(state: &RepositoryState, args: &Value) -> Result<Value, String> {
    let value = match configured_contract(
        state,
        "Create and verify an architecture contract before explaining violations.",
    )? {
        LoadedContract::Configured(value) => value,
        LoadedContract::NotConfigured(response) => return Ok(response),
    };
    let fingerprint = arg_str(args, "fingerprint")?;
    let Some(violation) = active_violation(state, &value, fingerprint)? else {
        return Ok(json!({
            "state": "NOT_FOUND",
            "fingerprint": fingerprint,
            "reason": "the fingerprint is not an active architecture violation"
        }));
    };
    Ok(json!({"violation": violation, "contract": ".weavatrix/architecture.json"}))
}

pub fn propose_exception(state: &RepositoryState, args: &Value) -> Result<Value, String> {
    let value = match configured_contract(
        state,
        "Create and verify an architecture contract before proposing exceptions.",
    )? {
        LoadedContract::Configured(value) => value,
        LoadedContract::NotConfigured(response) => return Ok(response),
    };
    let fingerprint = arg_str(args, "fingerprint")?;
    let reason = arg_str(args, "reason")?;
    let Some(violation) = active_violation(state, &value, fingerprint)? else {
        return Ok(json!({
            "state": "NOT_FOUND",
            "fingerprint": fingerprint,
            "reason": "only an active architecture violation can be proposed as an exception"
        }));
    };
    Ok(json!({
        "state": "PROPOSAL_ONLY",
        "proposal": {
            "fingerprint": fingerprint,
            "reason": reason,
            "expires": args.get("expires"),
            "violation": violation
        },
        "write": "NONE"
    }))
}

fn accepted_exceptions(value: &Value) -> BTreeSet<String> {
    value
        .get("exceptions")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter(|exception| {
            exception.get("expires").is_none() || exception["active"] == Value::Bool(true)
        })
        .filter_map(|exception| exception.get("fingerprint")?.as_str().map(str::to_owned))
        .collect()
}

fn all_violations(state: &RepositoryState, value: &Value) -> Result<Vec<Value>, String> {
    let mut violations = rules::dependency_violations(state, value);
    violations.extend(budgets::violations(state, value)?);
    violations.sort_by(|left, right| {
        left["fingerprint"]
            .as_str()
            .cmp(&right["fingerprint"].as_str())
    });
    Ok(violations)
}

fn active_violation(
    state: &RepositoryState,
    value: &Value,
    fingerprint: &str,
) -> Result<Option<Value>, String> {
    Ok(all_violations(state, value)?
        .into_iter()
        .find(|item| item["fingerprint"] == fingerprint))
}

enum LoadedContract {
    Configured(Value),
    NotConfigured(Value),
}

fn configured_contract(state: &RepositoryState, reason: &str) -> Result<LoadedContract, String> {
    Ok(match contract::load_optional(state)? {
        Some(value) => LoadedContract::Configured(value),
        None => LoadedContract::NotConfigured(contract::not_configured(state, reason)),
    })
}
