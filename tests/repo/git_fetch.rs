//! `git_fetch` clones one ref into a cache. The process root stays put.

use crate::support::GitFixture;
use blazingly_json::{Value, json};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};
use weavatrix_rust::{Weavatrix, tools};

static SEQUENCE: AtomicU64 = AtomicU64::new(0);

#[test]
fn fetches_one_ref_into_a_cache_and_leaves_the_process_root() {
    let pinned = GitFixture::new();
    pinned.write("PINNED.txt", "pinned-root\n");
    pinned.commit("pinned-root");
    let remote = TempDir::new("remote");
    write_remote(&remote.0);
    let mut engine = Weavatrix::open(&pinned.root).unwrap();
    refuse_bad_arguments(&mut engine, &remote.0);

    let main = tools::call(
        &mut engine,
        "git_fetch",
        json!({"url": remote.0.to_string_lossy(), "ref": "main"}),
    )
    .unwrap();
    let checkout = PathBuf::from(main["checkout"].as_str().expect("checkout"));
    let _cleanup = TempDir(checkout.clone());
    assert_eq!(main["status"], "COMPLETE");
    assert_eq!(main["ref"], "main");
    assert_eq!(main["depth"], 1);
    assert_eq!(main["reused"], false);
    assert_eq!(
        main["commit"],
        git(&remote.0, &["rev-parse", "main"]).as_str()
    );
    assert_same_path(&main["active_root"], &pinned.root);
    assert_same_path(&main["repository_context"]["root"], &pinned.root);
    assert_ne!(canon(&checkout), canon(&pinned.root));
    assert!(checkout.join("app/api/user/route.ts").is_file());
    assert_eq!(git(&checkout, &["rev-list", "--count", "HEAD"]), "1");
    assert_eq!(git(&remote.0, &["rev-list", "--count", "main"]), "2");
    assert_eq!(
        fs::read_to_string(pinned.root.join("PINNED.txt")).unwrap(),
        "pinned-root\n"
    );

    let feature = tools::call(
        &mut engine,
        "git_fetch",
        json!({"url": remote.0.to_string_lossy(), "ref": "feature"}),
    )
    .unwrap();
    assert_eq!(feature["reused"], true);
    assert_eq!(feature["checkout"], main["checkout"]);
    assert_eq!(
        feature["commit"],
        git(&remote.0, &["rev-parse", "feature"]).as_str()
    );
    assert!(checkout.join("feature.txt").is_file());
    assert!(!checkout.join("app/api/user/route.ts").exists());
    assert_same_path(&feature["repository_context"]["root"], &pinned.root);

    let head = tools::call(
        &mut engine,
        "git_fetch",
        json!({"url": remote.0.to_string_lossy()}),
    )
    .unwrap();
    assert_eq!(head["ref"], "HEAD");
    assert_eq!(head["reused"], true);
    assert_eq!(
        head["commit"],
        git(&remote.0, &["rev-parse", "HEAD"]).as_str()
    );
    assert!(checkout.join("app/api/user/route.ts").is_file());

    let mut analyzed = Weavatrix::open(&checkout).unwrap();
    let endpoints =
        tools::call(&mut analyzed, "list_endpoints", json!({"max_results": 20})).unwrap();
    let labels = endpoints["endpoints"]
        .as_array()
        .expect("endpoints")
        .iter()
        .map(|node| node["label"].as_str().unwrap_or("").to_owned())
        .collect::<Vec<_>>();
    assert_eq!(labels, vec!["GET /api/user".to_owned()], "{endpoints}");
    assert_launch_history(&mut engine, &pinned.root);
}

fn refuse_bad_arguments(engine: &mut Weavatrix, remote: &Path) {
    let missing = tools::call(engine, "git_fetch", json!({})).unwrap_err();
    assert!(missing.contains("url"), "{missing}");
    let flagged = tools::call(
        engine,
        "git_fetch",
        json!({"url": "-https://github.com/org/repo"}),
    )
    .unwrap_err();
    assert!(flagged.contains("'-'"), "{flagged}");
    let credential = tools::call(
        engine,
        "git_fetch",
        json!({"url": "https://token@github.com/org/repo"}),
    )
    .unwrap_err();
    assert!(credential.contains("credentials"), "{credential}");
    let refspec = tools::call(
        engine,
        "git_fetch",
        json!({"url": remote.to_string_lossy(), "ref": "main:refs/heads/main"}),
    )
    .unwrap_err();
    assert!(refspec.contains("branch, tag, or commit"), "{refspec}");
    let extra = tools::call(
        engine,
        "git_fetch",
        json!({"url": remote.to_string_lossy(), "depth": 1}),
    )
    .unwrap_err();
    assert!(extra.contains("depth"), "{extra}");
}

#[cfg(feature = "git")]
#[test]
fn git_history_treats_a_missing_parent_as_the_history_boundary() {
    let fixture = GitFixture::new();
    fixture.write("src/lib.rs", "pub fn first() {}\n");
    fixture.commit("first");
    let parent = git(&fixture.root, &["rev-parse", "HEAD"]);
    fixture.write("src/lib.rs", "pub fn second() {}\n");
    fixture.commit("second");
    let head = git(&fixture.root, &["rev-parse", "HEAD"]);
    let mut engine = Weavatrix::open(&fixture.root).unwrap();
    let full = tools::call(&mut engine, "git_history", json!({"max_commits": 10})).unwrap();
    assert_eq!(full["history_truncated"], false, "{full}");
    assert_eq!(full["commits"].as_array().map(Vec::len), Some(2), "{full}");

    let object = loose_object(&fixture.root, &parent);
    assert!(object.is_file(), "loose parent {}", object.display());
    fs::remove_file(&object).unwrap();
    let history = tools::call(&mut engine, "git_history", json!({"max_commits": 10})).unwrap();
    assert_eq!(history["history_truncated"], true, "{history}");
    let ids = commit_ids(&history);
    assert!(ids.contains(&head), "{history}");
    assert!(!ids.contains(&parent), "{history}");
    let by_path = tools::call(
        &mut engine,
        "git_history",
        json!({"path": "src/lib.rs", "max_commits": 10}),
    )
    .unwrap();
    assert_eq!(by_path["status"], "COMPLETE", "{by_path}");
    assert_eq!(by_path["history_truncated"], true, "{by_path}");
    let missing = tools::call(
        &mut engine,
        "git_history",
        json!({"revision": "0123456789abcdef0123456789abcdef01234567"}),
    )
    .unwrap_err();
    assert!(missing.contains("Git object not found"), "{missing}");
}

#[cfg(feature = "git")]
fn loose_object(root: &Path, id: &str) -> PathBuf {
    let (prefix, rest) = id.split_at(2);
    root.join(".git").join("objects").join(prefix).join(rest)
}

#[cfg(feature = "git")]
fn commit_ids(history: &Value) -> Vec<String> {
    history["commits"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|commit| commit["id"].as_str().map(str::to_owned))
        .collect()
}

fn assert_launch_history(engine: &mut Weavatrix, pinned: &Path) {
    #[cfg(feature = "git")]
    {
        let history = tools::call(
            engine,
            "git_history",
            json!({"max_commits": 5, "months": 120}),
        )
        .unwrap();
        let summaries = history["commits"]
            .as_array()
            .expect("commits")
            .iter()
            .filter_map(|commit| commit["summary"].as_str())
            .collect::<Vec<_>>();
        assert!(
            summaries.contains(&"pinned-root"),
            "git_history stays on the launch root: {history}"
        );
        assert_same_path(&history["repository_context"]["root"], pinned);
    }
    #[cfg(not(feature = "git"))]
    {
        let _ = (engine, pinned);
    }
}

fn write_remote(root: &Path) {
    git(root, &["init", "-q", "-b", "main"]);
    fs::write(root.join("README.md"), "one\n").unwrap();
    git(root, &["add", "-A"]);
    git(root, &["commit", "-qm", "first"]);
    fs::create_dir_all(root.join("app/api/user")).unwrap();
    fs::write(
        root.join("app/api/user/route.ts"),
        "export async function GET() {\n  return 1;\n}\n",
    )
    .unwrap();
    git(root, &["add", "-A"]);
    git(root, &["commit", "-qm", "route"]);
    git(root, &["checkout", "-q", "-b", "feature"]);
    fs::remove_file(root.join("app/api/user/route.ts")).unwrap();
    fs::write(root.join("feature.txt"), "feature\n").unwrap();
    git(root, &["add", "-A"]);
    git(root, &["commit", "-qm", "feature"]);
    git(root, &["checkout", "-q", "main"]);
}

fn git(cwd: &Path, args: &[&str]) -> String {
    let output = Command::new("git")
        .args(args)
        .current_dir(cwd)
        .env("GIT_AUTHOR_NAME", "Test")
        .env("GIT_AUTHOR_EMAIL", "test@example.invalid")
        .env("GIT_COMMITTER_NAME", "Test")
        .env("GIT_COMMITTER_EMAIL", "test@example.invalid")
        .output()
        .expect("git");
    assert!(
        output.status.success(),
        "{args:?}\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8_lossy(&output.stdout).trim().to_owned()
}

fn assert_same_path(value: &Value, expected: &Path) {
    assert_eq!(canon_value(value), canon(expected), "{value}");
}

fn canon_value(value: &Value) -> PathBuf {
    canon(Path::new(value.as_str().expect("path string")))
}

fn canon(path: &Path) -> PathBuf {
    path.canonicalize().unwrap_or_else(|_| path.to_path_buf())
}

struct TempDir(PathBuf);

impl TempDir {
    fn new(label: &str) -> Self {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "weavatrix-fetch-{label}-{}-{nanos}-{}",
            std::process::id(),
            SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&root).unwrap();
        Self(root)
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).ok();
    }
}
