use crate::language_fixture::Fixture;
use blazingly_json::json;
use weavatrix_rust::{Weavatrix, tools};

#[test]
fn graph_stats_names_edge_evidence_and_the_loaded_snapshot() {
    let fixture = Fixture::new();
    fixture.write("src/lib.rs", "pub fn delay() -> i32 { 1 }\n");
    let mut engine = Weavatrix::open(&fixture.root).unwrap();
    let stats = tools::call(&mut engine, "graph_stats", json!({})).unwrap();
    assert_eq!(stats["freshness"]["state"], "CURRENT", "{stats}");
    assert_eq!(stats["freshness"]["basis"], "loaded_snapshot", "{stats}");
    assert_eq!(stats["evidence_unit"], "edges", "{stats}");
    let share = stats["evidence_share"].as_object().expect("share");
    let sum: f64 = share.values().filter_map(|value| value.as_f64()).sum();
    assert!(
        stats["edges"].as_u64().unwrap_or(0) == 0 || (sum - 1.0).abs() < 0.02,
        "{stats}"
    );
    assert!(stats["evidence"]["parsed"].is_number() || stats["edges"].as_u64() == Some(0));
}

#[test]
fn a_worktree_copy_is_not_a_second_symbol() {
    let fixture = Fixture::new();
    fixture.write("src/lib.rs", "pub fn delay() -> i32 { 1 }\n");
    fixture.write(
        ".worktrees/copy/src/lib.rs",
        "pub fn delay() -> i32 { 2 }\n",
    );
    fixture.write(
        ".claude/worktrees/other/src/lib.rs",
        "pub fn delay() -> i32 { 3 }\n",
    );
    let mut engine = Weavatrix::open(&fixture.root).unwrap();
    let node = tools::call(&mut engine, "get_node", json!({"label": "delay"})).unwrap();
    let file = node["node"]["span"]["file"].as_str().unwrap_or("");
    assert!(!file.contains(".worktrees"), "{node}");
    assert!(!file.contains("worktrees"), "{node}");
}

#[test]
fn god_nodes_skip_files_constants_and_stylesheets() {
    let fixture = Fixture::new();
    fixture.write(
        "src/lib.rs",
        "pub fn real_hub() -> i32 { 1 }\nconst String: i32 = 1;\n",
    );
    fixture.write("web/theme.css", "body { color: red; }\n");
    let mut engine = Weavatrix::open(&fixture.root).unwrap();
    let hubs = tools::call(&mut engine, "god_nodes", json!({"top_n": 20})).unwrap();
    let rows = hubs["hubs"].as_array().expect("hubs");
    assert!(
        rows.iter().any(|row| row["node"]["label"] == "real_hub"),
        "{hubs}"
    );
    for row in rows {
        let label = row["node"]["label"].as_str().unwrap_or("");
        let kind = row["node"]["kind"].as_str().unwrap_or("");
        let file = row["node"]["span"]["file"].as_str().unwrap_or("");
        assert_ne!(label, "String", "{row}");
        assert!(!file.ends_with(".css"), "{row}");
        assert!(
            !matches!(
                kind,
                "file" | "module" | "unknown" | "constant" | "config_key" | "repository" | "package"
            ),
            "{row}"
        );
    }
}
