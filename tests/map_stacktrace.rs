mod language_fixture;

use blazingly_json::json;
use language_fixture::Fixture;
use weavatrix_rust::{Weavatrix, tools};

#[test]
fn node_frames_resolve_to_repository_symbols_and_classify_foreign_code() {
    let fixture = Fixture::new();
    fixture.write(
        "src/services/init.js",
        "export function initGraphs(){\n  return connect();\n}\nexport function connect(){ return 1; }\n",
    );
    let mut engine = Weavatrix::open(&fixture.root).unwrap();
    let trace = "TypeError: boom\n\
        at initGraphs (C:\\work\\emaildist\\src\\services\\init.js:1:17)\n\
        at run (C:\\work\\emaildist\\node_modules\\runner\\index.js:9:3)\n\
        at node:internal/main/run_main_module:28:49\n";

    let report = tools::call(&mut engine, "map_stacktrace", json!({"text": trace})).unwrap();
    let frames = report["frames"].as_array().unwrap();

    assert_eq!(report["total_frames"], 3);
    assert_eq!(report["resolved_frames"], 1);
    assert_eq!(frames[0]["classification"], "repository");
    assert_eq!(frames[0]["file"], "src/services/init.js");
    assert_eq!(frames[0]["node"]["label"], "initGraphs");
    assert_eq!(frames[0]["symbol_match"], "name");
    assert_eq!(frames[1]["classification"], "dependency");
    assert_eq!(frames[1]["resolved"], false);
    assert_eq!(frames[2]["classification"], "runtime");
}

#[test]
fn jvm_frames_use_the_package_convention_to_disambiguate_short_names() {
    let fixture = Fixture::new();
    fixture.write(
        "src/main/java/com/example/OrderService.java",
        "package com.example;\n\npublic class OrderService {\n    public String list() {\n        return \"orders\";\n    }\n}\n",
    );
    fixture.write(
        "src/test/java/com/other/OrderService.java",
        "package com.other;\n\npublic class OrderService {\n    public String list() {\n        return \"other\";\n    }\n}\n",
    );
    let mut engine = Weavatrix::open(&fixture.root).unwrap();
    let trace = "java.lang.IllegalStateException: boom\n\
        at com.example.OrderService.list(OrderService.java:4)\n\
        at java.base/java.lang.Thread.run(Thread.java:1589)\n";

    let report = tools::call(&mut engine, "map_stacktrace", json!({"text": trace})).unwrap();
    let frames = report["frames"].as_array().unwrap();

    assert_eq!(
        frames[0]["file"], "src/main/java/com/example/OrderService.java",
        "the com.example package narrows two OrderService.java files: {frames:?}"
    );
    assert_eq!(frames[0]["node"]["label"], "list");
    assert_eq!(frames[1]["classification"], "runtime");
}

#[test]
fn rust_panics_and_backtraces_map_to_files_and_symbols() {
    let fixture = Fixture::new();
    fixture.write("src/lib.rs", "pub fn boom() {\n    panic!(\"boom\");\n}\n");
    let mut engine = Weavatrix::open(&fixture.root).unwrap();
    let trace = "thread 'main' panicked at src/lib.rs:2:5:\nboom\n\
        stack backtrace:\n\
        3: fixture::boom\n\
        at ./src/lib.rs:2:5\n\
        4: std::panicking::begin_panic_handler\n\
        at /rustc/abc123/library/std/src/panicking.rs:665:5\n";

    let report = tools::call(&mut engine, "map_stacktrace", json!({"text": trace})).unwrap();
    let frames = report["frames"].as_array().unwrap();

    assert_eq!(frames[0]["file"], "src/lib.rs");
    assert_eq!(frames[0]["line"], 2);
    assert_eq!(frames[1]["file"], "src/lib.rs");
    assert_eq!(
        frames[1]["node"]["label"], "boom",
        "the numbered frame symbol attaches to the following at-line: {frames:?}"
    );
    assert_eq!(frames[2]["classification"], "runtime");
}

#[test]
fn python_tracebacks_resolve_and_the_text_contract_is_enforced() {
    let fixture = Fixture::new();
    fixture.write(
        "tasks/report.py",
        "def build_report():\n    return render()\n\ndef render():\n    return 1\n",
    );
    let mut engine = Weavatrix::open(&fixture.root).unwrap();
    let trace = "Traceback (most recent call last):\n\
        File \"/app/tasks/report.py\", line 2, in build_report\n\
        File \"/usr/lib/python3.12/site-packages/celery/app.py\", line 10, in run\n";

    let report = tools::call(&mut engine, "map_stacktrace", json!({"text": trace})).unwrap();
    let frames = report["frames"].as_array().unwrap();
    assert_eq!(frames[0]["file"], "tasks/report.py");
    assert_eq!(frames[0]["node"]["label"], "build_report");
    assert_eq!(frames[1]["classification"], "dependency");

    assert!(
        tools::call(&mut engine, "map_stacktrace", json!({}))
            .unwrap_err()
            .contains("text"),
        "text is a required argument"
    );
    let empty = tools::call(
        &mut engine,
        "map_stacktrace",
        json!({"text": "no frames here"}),
    )
    .unwrap();
    assert_eq!(empty["total_frames"], 0);
}
