use crate::support::GitFixture;
use std::process::Command;

fn cli() -> Command {
    Command::new(env!("CARGO_BIN_EXE_weavatrix-rust"))
}

#[test]
fn standalone_cli_reports_the_engine_identity() {
    let output = cli().arg("--version").output().expect("start");
    assert!(output.status.success());
    assert_eq!(
        String::from_utf8(output.stdout).expect("utf-8"),
        format!("weavatrix-rust {}\n", env!("CARGO_PKG_VERSION"))
    );
    assert!(output.stderr.is_empty());
}

#[test]
fn help_and_unknown_command_print_usage() {
    let help = cli().arg("-h").output().expect("start");
    assert!(help.status.success());
    let text = String::from_utf8(help.stdout).unwrap();
    assert!(text.contains("analyze"));
    assert!(text.contains("list-tools"));
    let unknown = cli().arg("wat").output().expect("start");
    assert!(!unknown.status.success());
    assert!(String::from_utf8_lossy(&unknown.stderr).contains("expected"));
}

#[test]
fn analyze_legacy_format_and_rejects_unknown_flags() {
    let fixture = GitFixture::new();
    fixture.write("src/app.js", "export const value = 1;\n");
    let legacy = cli()
        .args([
            "analyze",
            fixture.root.to_str().unwrap(),
            "--pretty",
            "--format=legacy",
        ])
        .output()
        .expect("start");
    assert!(legacy.status.success(), "{legacy:?}");
    assert!(String::from_utf8_lossy(&legacy.stdout).contains("nodes"));
    let bad_format = cli()
        .args(["analyze", fixture.root.to_str().unwrap(), "--format=nope"])
        .output()
        .expect("start");
    assert!(!bad_format.status.success());
    let bad_flag = cli()
        .args(["analyze", fixture.root.to_str().unwrap(), "--wat"])
        .output()
        .expect("start");
    assert!(!bad_flag.status.success());
}

#[test]
fn report_writes_the_three_owned_files() {
    let fixture = GitFixture::new();
    fixture.write("src/app.js", "export const value = 1;\n");
    let out = fixture.root.join("out");
    let report = cli()
        .args([
            "report",
            fixture.root.to_str().unwrap(),
            &format!("--out={}", out.display()),
        ])
        .output()
        .expect("start");
    assert!(report.status.success(), "{report:?}");
    assert!(out.join("index.html").is_file());
    assert!(out.join("REPORT.md").is_file());
    assert!(out.join("data.json").is_file());
    let bad = cli()
        .args(["report", fixture.root.to_str().unwrap(), "--wat"])
        .output()
        .expect("start");
    assert!(!bad.status.success());
}

#[test]
fn list_tools_prints_the_catalog() {
    let output = cli().arg("list-tools").output().expect("start");
    assert!(output.status.success());
    assert!(String::from_utf8_lossy(&output.stdout).contains("graph_stats"));
}
