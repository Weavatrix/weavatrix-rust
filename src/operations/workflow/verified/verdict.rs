use super::model::{Assessment, TestEvidence, VerificationChecks};
use blazingly_json::Value;

pub(super) fn assess(
    phase: &str,
    files: &[String],
    checks: &VerificationChecks,
    tests: &TestEvidence,
) -> Assessment {
    let mut blockers = Vec::new();
    let mut limitations = Vec::new();
    if phase == "plan" {
        limitations.push(
            "verification has not run; apply the edit and call verified_change with phase=verify"
                .to_owned(),
        );
    } else if !files.is_empty() {
        assess_repository_checks(checks, &mut blockers, &mut limitations);
        assess_tests(tests, &mut limitations);
    }
    let verdict = if blockers.is_empty() {
        if phase == "plan" {
            "PLANNED"
        } else if limitations.is_empty() {
            "PASS"
        } else {
            "REVIEW"
        }
    } else {
        "BLOCKED"
    };
    Assessment {
        verdict,
        blockers,
        limitations,
    }
}

fn assess_repository_checks(
    checks: &VerificationChecks,
    blockers: &mut Vec<String>,
    limitations: &mut Vec<String>,
) {
    if checks.audit["status"] == "REVIEW"
        || checks
            .audit
            .pointer("/debt/counts/new")
            .and_then(Value::as_u64)
            .is_some_and(|count| count > 0)
    {
        blockers.push("new Health findings or repository diagnostics were found".to_owned());
    }
    match checks.architecture["state"].as_str().unwrap_or("FAILED") {
        "BLOCKED" => blockers.push("new architecture-contract violations were found".to_owned()),
        "PASS" | "NOT_APPLICABLE" => {}
        _ => limitations.push(
            "architecture contract is not configured or verification is incomplete".to_owned(),
        ),
    }
    if checks.duplicates["state"] == "REVIEW" {
        limitations.push("duplicate ratchet requires review".to_owned());
    }
    if checks.api_contract["state"] == "REVIEW" {
        blockers.push("cross-repository API contract mismatches were found".to_owned());
    }
}

fn assess_tests(tests: &TestEvidence, limitations: &mut Vec<String>) {
    if !tests.requested.is_empty() {
        limitations.push("requested tests were not executed by the process-free core".to_owned());
    } else if !tests.suggested.is_empty() {
        limitations.push("affected tests were identified but not executed".to_owned());
    }
}
