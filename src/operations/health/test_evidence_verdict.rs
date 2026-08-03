//! Failure signals and verdict rendering for external test-run evidence.

use blazingly_json::{Value, json};

pub(super) struct Signals {
    counts: Counts,
    success: Option<bool>,
    exit_code: Option<i64>,
    timed_out: bool,
    teardown_timeout: bool,
    open_handles: usize,
    suite_status_failed: bool,
    messages: Vec<String>,
}

impl Signals {
    pub(super) fn collect(evidence: &Value, result: &Value) -> Self {
        let messages = messages(evidence, result);
        let normalized = messages.join("\n").to_ascii_lowercase();
        let teardown_timeout = (normalized.contains("afterall")
            || normalized.contains("after all"))
            && (normalized.contains("timeout")
                || normalized.contains("timed out")
                || normalized.contains("exceeded"));
        Self {
            counts: Counts::from(evidence, result),
            success: boolean(evidence, result, "success"),
            exit_code: integer(evidence, result, "exitCode")
                .or_else(|| integer(evidence, result, "exit_code")),
            timed_out: boolean(evidence, result, "timedOut")
                .or_else(|| boolean(evidence, result, "timed_out"))
                .unwrap_or(false),
            teardown_timeout,
            open_handles: open_handle_count(evidence, result, &normalized),
            suite_status_failed: result["testResults"]
                .as_array()
                .into_iter()
                .flatten()
                .any(|suite| suite["status"].as_str() == Some("failed")),
            messages,
        }
    }

    fn failed(&self) -> bool {
        self.exit_code.is_some_and(|code| code != 0)
            || self.success == Some(false)
            || self.counts.failed_suites > 0
            || self.counts.runtime_error_suites > 0
            || self.counts.failed_tests > 0
            || self.timed_out
            || self.teardown_timeout
            || self.open_handles > 0
            || self.suite_status_failed
    }

    fn status(&self) -> &'static str {
        if self.failed() {
            "FAIL"
        } else if self.exit_code == Some(0) && self.success == Some(true) {
            "PASS"
        } else {
            "INCOMPLETE"
        }
    }
}

fn findings(signals: &Signals) -> Vec<Value> {
    let mut findings = Vec::new();
    if signals.timed_out {
        findings.push(finding(
            "test.execution_timeout",
            "external test execution timed out",
            &signals.messages,
        ));
    }
    if signals.teardown_timeout {
        findings.push(finding(
            "test.teardown_timeout",
            "test teardown failed or timed out after assertions completed",
            &signals.messages,
        ));
    }
    if signals.open_handles > 0 {
        findings.push(json!({
            "id": "test.open_handles",
            "rule": "test.open_handles",
            "category": "tests",
            "severity": "high",
            "message": "external test evidence reports asynchronous handles left open",
            "open_handles": signals.open_handles
        }));
    }
    if signals.failed()
        && !signals.timed_out
        && !signals.teardown_timeout
        && signals.open_handles == 0
        || signals.counts.failed_suites > 0
        || signals.counts.runtime_error_suites > 0
        || signals.counts.failed_tests > 0
        || signals.exit_code.is_some_and(|code| code != 0)
    {
        findings.push(json!({
            "id": "test.execution_failed",
            "rule": "test.execution_failed",
            "category": "tests",
            "severity": "high",
            "message": "external test process or suite failed",
            "exit_code": signals.exit_code,
            "failed_suites": signals.counts.failed_suites,
            "failed_tests": signals.counts.failed_tests,
            "runtime_error_suites": signals.counts.runtime_error_suites
        }));
    }
    findings
}

pub(super) fn render(
    evidence: &Value,
    signals: &Signals,
    source: &str,
    revision: &str,
    max: usize,
) -> Value {
    let status = signals.status();
    let reason = if status == "INCOMPLETE" {
        json!("evidence lacks both a zero process exit code and an explicit successful result")
    } else {
        Value::Null
    };
    let mut findings = findings(signals);
    let findings_total = findings.len();
    findings.truncate(max);
    json!({
        "schemaVersion": super::SCHEMA,
        "status": status,
        "execution": {
            "present": true,
            "status": if status == "PASS" {"PASSED"} else if status == "FAIL" {"FAILED"} else {"INCOMPLETE"},
            "source": source,
            "repository_revision": revision,
            "revision_match": true,
            "exit_code": signals.exit_code,
            "timed_out": signals.timed_out,
            "reason": reason
        },
        "framework": evidence["framework"].as_str().unwrap_or("jest"),
        "suites": {
            "total": signals.counts.total_suites,
            "passed": signals.counts.passed_suites,
            "failed": signals.counts.failed_suites,
            "runtime_errors": signals.counts.runtime_error_suites
        },
        "assertions": {
            "total": signals.counts.total_tests,
            "passed": signals.counts.passed_tests,
            "failed": signals.counts.failed_tests,
            "pending": signals.counts.pending_tests
        },
        "open_handles": signals.open_handles,
        "findings_total": findings_total,
        "findings": findings
    })
}

#[derive(Default)]
struct Counts {
    total_suites: u64,
    passed_suites: u64,
    failed_suites: u64,
    runtime_error_suites: u64,
    total_tests: u64,
    passed_tests: u64,
    failed_tests: u64,
    pending_tests: u64,
}

impl Counts {
    fn from(evidence: &Value, result: &Value) -> Self {
        Self {
            total_suites: number(evidence, result, "numTotalTestSuites"),
            passed_suites: number(evidence, result, "numPassedTestSuites"),
            failed_suites: number(evidence, result, "numFailedTestSuites"),
            runtime_error_suites: number(evidence, result, "numRuntimeErrorTestSuites"),
            total_tests: number(evidence, result, "numTotalTests"),
            passed_tests: number(evidence, result, "numPassedTests"),
            failed_tests: number(evidence, result, "numFailedTests"),
            pending_tests: number(evidence, result, "numPendingTests"),
        }
    }
}

fn boolean(left: &Value, right: &Value, key: &str) -> Option<bool> {
    left[key].as_bool().or_else(|| right[key].as_bool())
}

fn integer(left: &Value, right: &Value, key: &str) -> Option<i64> {
    left[key].as_i64().or_else(|| right[key].as_i64())
}

fn number(left: &Value, right: &Value, key: &str) -> u64 {
    left[key]
        .as_u64()
        .or_else(|| right[key].as_u64())
        .unwrap_or(0)
}

fn messages(evidence: &Value, result: &Value) -> Vec<String> {
    let mut out = Vec::new();
    for value in [
        evidence.get("message"),
        evidence.get("stderr"),
        result.get("message"),
        result.get("runExecError"),
    ]
    .into_iter()
    .flatten()
    {
        if let Some(text) = value.as_str() {
            out.push(text.to_owned());
        } else if value.is_object() && value["message"].as_str().is_some() {
            out.push(value["message"].as_str().unwrap_or_default().to_owned());
        }
    }
    for suite in result["testResults"].as_array().into_iter().flatten() {
        for key in ["message", "failureMessage", "summary"] {
            if let Some(text) = suite[key].as_str() {
                out.push(text.to_owned());
            }
        }
    }
    out
}

fn open_handle_count(evidence: &Value, result: &Value, messages: &str) -> usize {
    for key in ["openHandles", "open_handles"] {
        if let Some(handles) = evidence[key].as_array().or_else(|| result[key].as_array()) {
            return handles.len();
        }
    }
    usize::from(messages.contains("open handle") || messages.contains("did not exit"))
}

fn finding(rule: &str, message: &str, messages: &[String]) -> Value {
    let evidence = messages
        .iter()
        .find(|text| !text.is_empty())
        .map(|text| text.chars().take(500).collect::<String>());
    json!({
        "id": rule,
        "rule": rule,
        "category": "tests",
        "severity": "high",
        "message": message,
        "evidence": evidence
    })
}
