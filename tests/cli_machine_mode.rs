//! The standalone CLI has to be drivable by an unattended loop: one-line
//! JSON, arguments that do not fight the shell's quoting, one field pulled
//! out for a results table, and an exit code that separates "the evidence
//! says no" from "the tool failed to run".

mod support;

use std::io::Write;
use std::process::{Command, Output, Stdio};
use support::GitFixture;

fn repository() -> GitFixture {
    let fixture = GitFixture::new();
    fixture.write(
        "lib/util.js",
        "export function helper() {\n  return 1;\n}\n",
    );
    fixture.write(
        "app/main.js",
        "import { helper } from '../lib/util.js';\nexport function list() {\n  return helper();\n}\n",
    );
    fixture.write(
        ".weavatrix/architecture.json",
        r#"{"components":[{"id":"app","paths":["app"]},{"id":"lib","paths":["lib"]}],
            "dependencyRules":[{"id":"no-app-lib","action":"forbid","from":["app"],
            "to":["lib"],"kinds":["imports"]}],"ratchet":{"baseline":{"fingerprints":[]}}}"#,
    );
    fixture.commit("baseline");
    fixture
}

fn cli(arguments: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_weavatrix-rust"))
        .args(arguments)
        .output()
        .expect("standalone CLI must start")
}

fn stdout_of(output: &Output) -> String {
    String::from_utf8(output.stdout.clone()).expect("stdout must be UTF-8")
}

#[test]
fn compact_output_is_one_appendable_line_of_json() {
    let fixture = repository();
    let root = fixture.root.to_string_lossy().to_string();
    let output = cli(&["tool", "graph_stats", &root, "--compact"]);

    assert!(output.status.success(), "{}", stdout_of(&output));
    let text = stdout_of(&output);
    assert_eq!(
        text.lines().count(),
        1,
        "a loop appends this to a log, so it must not wrap: {text}"
    );
    let parsed = blazingly_json::from_str::<blazingly_json::Value>(text.trim()).unwrap();
    assert_eq!(parsed["freshness"]["state"], "CURRENT");
}

#[test]
fn a_selected_field_prints_without_the_quotes_a_results_table_would_carry() {
    let fixture = repository();
    let root = fixture.root.to_string_lossy().to_string();

    let selected = cli(&["tool", "graph_stats", &root, "--select=/freshness/state"]);
    assert!(selected.status.success());
    assert_eq!(
        stdout_of(&selected),
        "CURRENT\n",
        "a string field is pasted into a column, not quoted"
    );

    let number = cli(&[
        "tool",
        "graph_stats",
        &root,
        "--select=/repository_context/graph_age_seconds",
    ]);
    assert!(number.status.success());
    assert!(
        stdout_of(&number).trim().parse::<u64>().is_ok(),
        "a numeric field prints as a bare number: {}",
        stdout_of(&number)
    );
}

#[test]
fn an_absent_pointer_fails_instead_of_printing_a_blank_measurement() {
    let fixture = repository();
    let root = fixture.root.to_string_lossy().to_string();
    let output = cli(&["tool", "graph_stats", &root, "--select=/no/such/field"]);

    assert_eq!(output.status.code(), Some(1));
    assert!(stdout_of(&output).is_empty(), "nothing is recorded");
    let error = String::from_utf8(output.stderr).unwrap();
    assert!(error.contains("/no/such/field"), "{error}");
}

#[test]
fn arguments_can_arrive_on_stdin_instead_of_through_the_shell() {
    let fixture = repository();
    let root = fixture.root.to_string_lossy().to_string();
    let mut child = Command::new(env!("CARGO_BIN_EXE_weavatrix-rust"))
        .args(["tool", "get_node", &root, "--stdin", "--select=/node/label"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("standalone CLI must start");
    child
        .stdin
        .take()
        .expect("stdin is piped")
        .write_all(br#"{"label": "helper"}"#)
        .unwrap();
    let output = child.wait_with_output().unwrap();

    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(stdout_of(&output), "helper\n");
}

#[test]
fn two_argument_sources_at_once_are_a_caller_error() {
    let fixture = repository();
    let root = fixture.root.to_string_lossy().to_string();
    let output = cli(&["tool", "graph_stats", &root, "{}", "--stdin"]);

    assert_eq!(output.status.code(), Some(1));
    let error = String::from_utf8(output.stderr).unwrap();
    assert!(error.contains("--stdin"), "{error}");
}

#[test]
fn a_blocked_gate_answers_on_stdout_and_reports_its_verdict_in_the_exit_code() {
    let fixture = repository();
    let root = fixture.root.to_string_lossy().to_string();
    let blocked = cli(&["tool", "verify_architecture", &root, "--compact"]);

    assert_eq!(
        blocked.status.code(),
        Some(2),
        "a failed gate is not a failed tool: {}",
        stdout_of(&blocked)
    );
    let parsed =
        blazingly_json::from_str::<blazingly_json::Value>(stdout_of(&blocked).trim()).unwrap();
    assert_eq!(parsed["state"], "BLOCKED");

    let missing = cli(&["tool", "no_such_operation", &root, "--compact"]);
    assert_eq!(
        missing.status.code(),
        Some(1),
        "an unknown operation is a failed tool, not a verdict"
    );
}
