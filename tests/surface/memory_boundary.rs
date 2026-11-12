#[test]
fn many_agents_share_one_process_and_a_warm_graph_is_reused() {
    use std::fs;
    use std::thread;

    use blazingly_json::json;
    use weavatrix_rust::ParallelEngine;

    let parent = scratch("parent");
    fs::create_dir_all(parent.join("shop/src")).unwrap();
    fs::write(parent.join("shop/src/lib.rs"), "pub fn shop() {}\n").unwrap();
    let outside = std::env::temp_dir().join(format!("weavatrix-outside-{}", std::process::id()));
    fs::create_dir_all(&outside).unwrap();
    let engine = ParallelEngine::open(&parent, false).expect("open");
    let launch = engine.launch().to_path_buf();

    thread::scope(|scope| {
        scope.spawn(|| {
            let shop = engine
                .call(
                    "graph_stats",
                    json!({"agent": "alpha", "repository": "shop"}),
                )
                .expect("shop");
            assert_eq!(
                canon(&shop["repository_context"]["root"]),
                canon_path(&parent.join("shop"))
            );
        });
        scope.spawn(|| {
            let root = engine
                .call("graph_stats", json!({"agent": "beta"}))
                .expect("parent");
            assert_eq!(canon(&root["repository_context"]["root"]), launch);
        });
    });
    engine
        .call(
            "graph_stats",
            json!({"agent": "alpha", "repository": "shop"}),
        )
        .expect("warm shop");
    let engine = &engine;
    thread::scope(|scope| {
        for index in 0..40 {
            scope.spawn(move || {
                engine
                    .call("graph_stats", json!({"agent": format!("lane-{index}")}))
                    .expect("lane");
            });
        }
    });

    let listed = engine.call("list_known_repos", json!({})).expect("log");
    let recent = listed["recent_requests"].as_array().expect("recent");
    assert!(recent.iter().any(|row| {
        row["agent"] == "alpha" && row["tool"] == "graph_stats" && row["reused"] == true
    }));
    assert!(recent.iter().any(|row| {
        row["at_utc"]
            .as_str()
            .is_some_and(|stamp| stamp.contains('T') && stamp.ends_with('Z'))
    }));
    assert!(listed["requests_total"].as_u64().unwrap_or(0) >= 42);
    assert_eq!(engine.launch(), launch.as_path());

    let escaped = engine
        .call(
            "graph_stats",
            json!({"repository": outside.display().to_string()}),
        )
        .expect_err("outside");
    assert!(escaped.contains("launch root"), "{escaped}");
    let refused = engine
        .call(
            "open_repo",
            json!({"path": parent.join("shop").display().to_string()}),
        )
        .expect_err("retarget");
    assert!(refused.contains("allow-retarget"), "{refused}");
    let pin = engine.call("graph_stats", json!({})).expect("pin");
    assert_eq!(canon(&pin["repository_context"]["root"]), launch);
    let _ = fs::remove_dir_all(&parent);
    let _ = fs::remove_dir_all(&outside);
}

fn scratch(name: &str) -> std::path::PathBuf {
    let root =
        std::env::temp_dir().join(format!("weavatrix-lanes-{}-{}", std::process::id(), name));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(root.join("src")).unwrap();
    std::fs::write(root.join("src/lib.rs"), "pub fn lane() {}\n").unwrap();
    root
}

fn canon(value: &blazingly_json::Value) -> std::path::PathBuf {
    canon_path(std::path::Path::new(value.as_str().unwrap_or_default()))
}

fn canon_path(path: &std::path::Path) -> std::path::PathBuf {
    path.canonicalize().unwrap_or_else(|_| path.to_path_buf())
}

#[cfg(feature = "memory")]
#[test]
fn rust_core_exposes_the_standalone_memory_module() {
    use weavatrix_rust::memory::{EntityId, Timestamp};

    let entity = EntityId::new("task:memory-boundary").unwrap();
    let timestamp = Timestamp::from_unix_micros(42);

    assert_eq!(entity.as_str(), "task:memory-boundary");
    assert_eq!(timestamp.as_unix_micros(), 42);
}
