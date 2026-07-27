#![cfg(feature = "git")]

use serde_json::json;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};
use weavatrix_rust::{Weavatrix, tools};

static SEQUENCE: AtomicU64 = AtomicU64::new(0);

#[test]
fn compares_analyzed_worktree_with_immutable_git_objects() {
    let root = fixture();
    fs::create_dir_all(root.join("src")).unwrap();
    fs::write(root.join("src/lib.rs"), "pub fn existing() {}\n").unwrap();
    git(&root, &["add", "-A"]);
    git(
        &root,
        &[
            "-c",
            "user.name=Test",
            "-c",
            "user.email=test@example.invalid",
            "commit",
            "-qm",
            "baseline",
        ],
    );
    fs::write(
        root.join("src/lib.rs"),
        "pub fn existing() {}\npub fn added() { existing(); }\n",
    )
    .unwrap();

    let mut engine = Weavatrix::open(&root).unwrap();
    let result = tools::call(
        &mut engine,
        "graph_diff",
        json!({"base_ref": "HEAD", "max_results": 20}),
    )
    .unwrap();

    assert_eq!(result["completeness"], "COMPLETE_FOR_SUPPORTED_LANGUAGES");
    assert_eq!(result["source_mutation"], "NONE");
    assert_eq!(result["git_process"], "NONE");
    assert!(result["counts"]["nodes_added"].as_u64().unwrap() >= 1);
    assert!(result["counts"]["nodes_changed"].as_u64().unwrap() >= 1);
    fs::remove_dir_all(root).ok();
}

fn fixture() -> PathBuf {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let root = std::env::temp_dir().join(format!(
        "weavatrix-rust-git-diff-{}-{unique}-{}",
        std::process::id(),
        SEQUENCE.fetch_add(1, Ordering::Relaxed)
    ));
    fs::create_dir_all(&root).unwrap();
    git(&root, &["init", "-q"]);
    root
}

fn git(path: &Path, args: &[&str]) {
    let output = Command::new("git")
        .args(args)
        .current_dir(path)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}
