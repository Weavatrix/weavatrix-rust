use blazingly_json::{Value, json};
use std::env;
use std::hint::black_box;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};
use weavatrix_rust::{Weavatrix, operations};

fn main() {
    let repositories = repositories();
    if repositories.is_empty() {
        // Cargo executes `harness = false` benches without a distinguishing
        // argument during `cargo test --all-targets`. Test profiles retain
        // debug assertions; an actual `cargo bench` uses the release profile
        // and must still fail closed when no corpus was supplied.
        if cfg!(debug_assertions) {
            eprintln!("repository benchmark skipped during `cargo test`: no corpus configured");
            return;
        }
        eprintln!("pass repository paths after `--` or set WEAVATRIX_BENCH_REPOSITORIES");
        std::process::exit(2);
    }
    let results = repositories
        .iter()
        .map(|path| benchmark(path))
        .collect::<Vec<_>>();
    let output = blazingly_json::to_string_pretty(&json!({
        "schema": "weavatrix.repository-benchmark.v1",
        "engine_version": env!("CARGO_PKG_VERSION"),
        "profile": "release",
        "samples": 3,
        "repositories": results
    }))
    .unwrap();
    if let Some(path) = env::var_os("WEAVATRIX_BENCH_OUTPUT") {
        std::fs::write(path, output).unwrap();
    } else {
        println!("{output}");
    }
}

fn repositories() -> Vec<PathBuf> {
    let arguments = env::args()
        .skip(1)
        .filter(|value| !value.starts_with('-'))
        .map(PathBuf::from)
        .collect::<Vec<_>>();
    if !arguments.is_empty() {
        return arguments;
    }
    env::var_os("WEAVATRIX_BENCH_REPOSITORIES")
        .map(|value| env::split_paths(&value).collect())
        .unwrap_or_default()
}

fn benchmark(path: &Path) -> Value {
    let mut cold = Vec::new();
    let mut last = None;
    for _ in 0..3 {
        let started = Instant::now();
        let engine = Weavatrix::open(path)
            .unwrap_or_else(|error| panic!("failed to analyze {}: {error}", path.display()));
        cold.push(started.elapsed());
        last = Some(engine);
    }
    let mut engine = last.unwrap();
    let stats = operations::call(&mut engine, "graph_stats", json!({})).unwrap();
    let stats_time = repeated(1_000, || {
        black_box(operations::call(&mut engine, "graph_stats", json!({})).unwrap());
    });
    let refresh_time = repeated(3, || {
        assert!(!black_box(engine.refresh_if_stale().unwrap()));
    });
    let search_time = repeated(5, || {
        black_box(
            operations::call(
                &mut engine,
                "search_code",
                json!({"query": "TODO", "max_results": 100}),
            )
            .unwrap(),
        );
    });
    cold.sort_unstable();
    json!({
        "repository": path.file_name().and_then(|name| name.to_str()).unwrap_or("repository"),
        "revision": revision(path),
        "cold_build_ms": durations(&cold),
        "hot_graph_stats_us": per_iteration(stats_time, 1_000) * 1_000.0,
        "unchanged_refresh_ms": per_iteration(refresh_time, 3),
        "literal_search_ms": per_iteration(search_time, 5),
        "files": stats["node_kinds"]["file"],
        "nodes": stats["nodes"],
        "edges": stats["edges"],
        "endpoints": stats["node_kinds"]["endpoint"],
        "tables": stats["node_kinds"]["table"],
        "topics": stats["node_kinds"]["topic"],
        "queues": stats["node_kinds"]["queue"],
        "collections": stats["node_kinds"]["collection"],
        "node_kinds": stats["node_kinds"],
        "relations": stats["relations"],
        "evidence": stats["evidence"]
    })
}

#[cfg(feature = "git")]
fn revision(path: &Path) -> Option<String> {
    weavatrix_git::Repository::open(path)
        .ok()?
        .resolve("HEAD")
        .ok()
        .map(|id| id.to_string())
}

#[cfg(not(feature = "git"))]
fn revision(_path: &Path) -> Option<String> {
    None
}

fn repeated(iterations: u32, mut operation: impl FnMut()) -> Duration {
    let started = Instant::now();
    for _ in 0..iterations {
        operation();
    }
    started.elapsed()
}

fn per_iteration(duration: Duration, iterations: u32) -> f64 {
    duration.as_secs_f64() * 1_000.0 / f64::from(iterations)
}

fn durations(samples: &[Duration]) -> Value {
    let milliseconds = samples
        .iter()
        .map(|duration| duration.as_secs_f64() * 1_000.0)
        .collect::<Vec<_>>();
    json!({
        "min": milliseconds[0],
        "median": milliseconds[milliseconds.len() / 2],
        "max": milliseconds[milliseconds.len() - 1]
    })
}
