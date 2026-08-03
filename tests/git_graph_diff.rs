#![cfg(feature = "git")]

use blazingly_json::json;
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
    assert_eq!(
        result["counts"]["nodes_changed"], 1,
        "only the file whose byte length changed is structurally different; hash formats must not mark every file"
    );
    assert_eq!(
        result["nodes"]["changed"][0]["after"]["id"],
        "file:src/lib.rs"
    );
    fs::remove_dir_all(root).ok();
}

#[test]
fn change_impact_uses_the_active_worktree_by_default() {
    let root = fixture();
    fs::create_dir_all(root.join("src")).unwrap();
    fs::write(root.join("src/lib.rs"), "pub fn existing() {}\n").unwrap();
    fs::write(root.join("src/removed.rs"), "pub fn removed() {}\n").unwrap();
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
        "pub fn existing() { changed(); }\npub fn changed() {}\n",
    )
    .unwrap();
    fs::remove_file(root.join("src/removed.rs")).unwrap();
    fs::write(root.join("src/added.rs"), "pub fn added() {}\n").unwrap();

    let mut engine = Weavatrix::open(&root).unwrap();
    let result = tools::call(&mut engine, "change_impact", json!({})).unwrap();
    let files = result["changed_files"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(blazingly_json::Value::as_str)
        .collect::<Vec<_>>();

    assert_eq!(result["git"]["head"], "WORKTREE");
    assert_eq!(
        files,
        ["src/added.rs", "src/lib.rs", "src/removed.rs"],
        "tracked, deleted and untracked supported sources are all included"
    );
    fs::remove_dir_all(root).ok();
}

#[test]
fn change_impact_returns_flat_dependent_nodes() {
    let root = fixture();
    fs::create_dir_all(root.join("services")).unwrap();
    fs::create_dir_all(root.join("tests")).unwrap();
    fs::write(
        root.join("services/init.js"),
        "export function initialize() {}\n",
    )
    .unwrap();
    fs::write(
        root.join("services/consumer.js"),
        "import { initialize } from './init.js';\nexport function start() { initialize(); }\n",
    )
    .unwrap();
    fs::write(
        root.join("tests/consumer.test.js"),
        "import { start } from '../services/consumer.js';\ntest('start', () => start());\n",
    )
    .unwrap();

    let mut engine = Weavatrix::open(&root).unwrap();
    let result = tools::call(
        &mut engine,
        "change_impact",
        json!({"files": ["services/init.js"], "depth": 2, "max_nodes": 20}),
    )
    .unwrap();
    let impacted = result["impacted_nodes"].as_array().unwrap();
    let ids = impacted
        .iter()
        .filter_map(|node| node["id"].as_str())
        .collect::<Vec<_>>();

    assert!(ids.contains(&"file:services/consumer.js"), "{impacted:?}");
    assert!(ids.contains(&"file:tests/consumer.test.js"), "{impacted:?}");
    assert!(
        impacted
            .iter()
            .all(|node| node.get("node").is_none() && node.get("id").is_some()),
        "change impact must expose the same flat node shape consumed by verified_change: {impacted:?}"
    );
    fs::remove_dir_all(root).ok();
}

#[test]
fn bounded_static_tools_reject_unavailable_lsp_precision() {
    let root = fixture();
    fs::create_dir_all(root.join("src")).unwrap();
    fs::write(root.join("src/value.js"), "export const value = 1;\n").unwrap();
    let mut engine = Weavatrix::open(&root).unwrap();

    for (tool, arguments) in [
        (
            "get_dependents",
            json!({"label": "file:src/value.js", "precision": "lsp"}),
        ),
        (
            "change_impact",
            json!({"files": ["src/value.js"], "precision": "lsp"}),
        ),
    ] {
        let error = tools::call(&mut engine, tool, arguments).unwrap_err();
        assert!(
            error.contains("supports only 'graph' bounded static precision"),
            "{tool} must fail instead of silently downgrading lsp: {error}"
        );
    }
    fs::remove_dir_all(root).ok();
}

#[test]
fn verified_change_passes_an_unchanged_worktree_without_running_processes() {
    let root = fixture();
    fs::create_dir_all(root.join("src")).unwrap();
    fs::write(root.join("src/lib.rs"), "pub fn stable() {}\n").unwrap();
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

    let mut engine = Weavatrix::open(&root).unwrap();
    let result = tools::call(
        &mut engine,
        "verified_change",
        json!({"task": "verify stable tree", "phase": "verify", "base_ref": "HEAD"}),
    )
    .unwrap();

    assert_eq!(result["verdict"], "PASS");
    assert_eq!(
        result["changeImpact"]["changed_files"]
            .as_array()
            .unwrap()
            .len(),
        0
    );
    assert_eq!(result["source_mutation"], "NONE");
    assert_eq!(result["test_execution"]["present"], false);
    assert_eq!(
        result["test_execution"]["reason"],
        "no test command was requested"
    );
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
