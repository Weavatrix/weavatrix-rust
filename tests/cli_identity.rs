use std::process::Command;

#[test]
fn standalone_cli_reports_the_engine_identity() {
    let output = Command::new(env!("CARGO_BIN_EXE_weavatrix-rust"))
        .arg("--version")
        .output()
        .expect("standalone CLI must start");

    assert!(output.status.success());
    assert_eq!(
        String::from_utf8(output.stdout).expect("version output must be UTF-8"),
        format!("weavatrix-rust {}\n", env!("CARGO_PKG_VERSION"))
    );
    assert!(output.stderr.is_empty());
}
