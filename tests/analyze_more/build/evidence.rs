use crate::language_fixture::Fixture;
use blazingly_json::json;
use weavatrix_rust::{Weavatrix, tools};

#[test]
#[cfg(unix)]
fn manifest_symlink_outside_the_repository_is_excluded() {
    use std::os::unix::fs::symlink;

    let fixture = Fixture::new();
    fixture.write("src/lib.rs", "pub fn value() {}\n");
    let outside = fixture.root.with_extension("outside-Cargo.toml");
    std::fs::write(&outside, "[package]\nname = 'outside'\nversion = '0.1.0'\n").unwrap();
    symlink(&outside, fixture.root.join("Cargo.toml")).unwrap();

    let mut engine = Weavatrix::open(&fixture.root).unwrap();
    let report = tools::call(&mut engine, "build_graph", json!({})).unwrap();
    assert_eq!(report["status"], "INCOMPLETE");
    assert!(report["workspaces"].as_array().unwrap().is_empty());
    assert!(
        report["input_capture"]["excluded"]
            .as_array()
            .unwrap()
            .iter()
            .any(|item| { item["path"] == "Cargo.toml" && item["reason"] == "symlink" })
    );

    let _ = std::fs::remove_file(outside);
}
