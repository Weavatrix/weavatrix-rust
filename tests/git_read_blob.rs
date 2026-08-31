//! `git_read_blob`: the file as it was at a revision, without a checkout.

#![cfg(feature = "git")]

#[allow(dead_code)]
mod support;

use blazingly_json::json;
use support::GitFixture;
use weavatrix_rust::{Weavatrix, tools};

#[test]
fn reads_a_committed_file_at_a_revision_and_by_oid() {
    let fixture = GitFixture::new();
    fixture.write("src/app.js", "export const version = 1;\n");
    fixture.commit("first");
    fixture.write("src/app.js", "export const version = 2;\n");
    fixture.commit("second");

    let mut engine = Weavatrix::open(&fixture.root).unwrap();
    let past = tools::call(
        &mut engine,
        "git_read_blob",
        json!({"path": "src/app.js", "revision": "HEAD~1"}),
    )
    .unwrap();
    assert_eq!(
        past["lines"],
        json!(["export const version = 1;"]),
        "the previous revision's content must be served: {past}"
    );
    assert_eq!(past["kind"], "utf8-text");
    assert_eq!(past["truncated"], false);

    let oid = past["oid"].as_str().unwrap().to_owned();
    let by_oid = tools::call(&mut engine, "git_read_blob", json!({"oid": oid})).unwrap();
    assert_eq!(
        by_oid["lines"], past["lines"],
        "addressing the same blob by oid returns the same content"
    );

    let current = tools::call(&mut engine, "git_read_blob", json!({"path": "src/app.js"})).unwrap();
    assert_eq!(
        current["lines"],
        json!(["export const version = 2;"]),
        "HEAD is the default revision: {current}"
    );
}

#[test]
fn binary_blobs_and_missing_paths_fail_closed() {
    let fixture = GitFixture::new();
    fixture.write("src/app.js", "export const version = 1;\n");
    std::fs::write(fixture.root.join("logo.bin"), [0_u8, 159, 146, 150, 255]).unwrap();
    fixture.commit("first");

    let mut engine = Weavatrix::open(&fixture.root).unwrap();
    let binary =
        tools::call(&mut engine, "git_read_blob", json!({"path": "logo.bin"})).unwrap_err();
    assert!(
        binary.contains("not UTF-8"),
        "binary content is refused, not decoded lossily: {binary}"
    );

    let missing =
        tools::call(&mut engine, "git_read_blob", json!({"path": "src/gone.js"})).unwrap_err();
    assert!(
        missing.contains("src/gone.js"),
        "a missing path names itself: {missing}"
    );

    let unaddressed = tools::call(&mut engine, "git_read_blob", json!({})).unwrap_err();
    assert!(
        unaddressed.contains("oid"),
        "the addressing contract is explained: {unaddressed}"
    );
}

#[test]
fn oversized_content_is_truncated_at_a_character_boundary() {
    let fixture = GitFixture::new();
    let long = "const banner = \"я строка\";\n".repeat(64);
    fixture.write("src/long.js", &long);
    fixture.commit("first");

    let mut engine = Weavatrix::open(&fixture.root).unwrap();
    let report = tools::call(
        &mut engine,
        "git_read_blob",
        json!({"path": "src/long.js", "max_bytes": 100}),
    )
    .unwrap();
    assert_eq!(report["truncated"], true);
    assert!(report["returned_bytes"].as_u64().unwrap() <= 100);
    assert_eq!(
        report["size_bytes"].as_u64().unwrap(),
        long.len() as u64,
        "the true size is reported alongside the truncation: {report}"
    );
}
