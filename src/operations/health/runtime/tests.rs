use super::rules::finding_id;
use super::runtime_findings;

#[test]
fn finding_identity_survives_line_shifts_and_reindentation() {
    let original = "    for (const item of items) { await save(item); }";
    let shifted = "        for (const item of items) {  await save(item); }";
    assert_eq!(
        finding_id("runtime.await_in_loop", "src/a.js", original),
        finding_id("runtime.await_in_loop", "src/a.js", shifted),
        "indentation and inner spacing must not change a finding identity"
    );
    assert_ne!(
        finding_id("runtime.await_in_loop", "src/a.js", original),
        finding_id("runtime.await_in_loop", "src/b.js", original),
        "the same code in another file is another finding"
    );
}

#[test]
fn runtime_rules_report_stable_findings_for_moved_code() {
    let body = "const items = [];\nfor (const item of items) { await save(item); }\n";
    let moved = format!("// header\n// header\n{body}");
    let (first, scanned, truncated) = runtime_findings(
        [(
            "src/a.js".to_owned(),
            "javascript".to_owned(),
            body.to_owned(),
        )],
        10,
    );
    let (second, _, _) = runtime_findings(
        [("src/a.js".to_owned(), "javascript".to_owned(), moved)],
        10,
    );
    assert_eq!(scanned, 1);
    assert!(!truncated);
    assert_eq!(first.len(), 1, "await inside a loop is reported once");
    assert_eq!(
        first[0]["id"], second[0]["id"],
        "moving code down the file keeps its identity so debt stays comparable"
    );
    assert_ne!(first[0]["line"], second[0]["line"]);
}

#[test]
fn runtime_rules_ignore_rust_code_compiled_only_for_tests() {
    let source = r#"
pub fn production() {
    risky().unwrap();
}

#[cfg(test)]
mod tests {
    const JSON: &str = "{ braces in strings do not close the module }";

    #[test]
    fn smoke() {
        risky().expect("tests may assert");
    }
}
"#;
    let (findings, scanned, truncated) = runtime_findings(
        [(
            "src/lib.rs".to_owned(),
            "rust".to_owned(),
            source.to_owned(),
        )],
        10,
    );
    assert_eq!(scanned, 1);
    assert!(!truncated);
    assert_eq!(findings.len(), 1);
    assert_eq!(findings[0]["line"], 3);
}

#[test]
fn blocking_calls_are_reported_only_inside_async_contexts() {
    let rust = r"
fn background_worker() {
    std::thread::sleep(std::time::Duration::from_millis(10));
}

async fn request_handler() {
    std::thread::sleep(std::time::Duration::from_millis(10));
}
";
    let python = r"
def background_worker():
    time.sleep(1)

async def request_handler():
    time.sleep(1)
";
    let javascript = r#"
function backgroundWorker() {
    readFileSync("state.json");
}

async function requestHandler() {
    readFileSync("state.json");
}
"#;
    let (findings, scanned, truncated) = runtime_findings(
        [
            ("src/lib.rs".to_owned(), "rust".to_owned(), rust.to_owned()),
            (
                "worker.py".to_owned(),
                "python".to_owned(),
                python.to_owned(),
            ),
            (
                "worker.js".to_owned(),
                "javascript".to_owned(),
                javascript.to_owned(),
            ),
        ],
        10,
    );
    assert_eq!(scanned, 3);
    assert!(!truncated);
    assert_eq!(findings.len(), 3);
    assert!(findings.iter().all(|finding| {
        let expected = if finding["file"] == "worker.py" { 6 } else { 7 };
        finding["line"] == expected
    }));
}
