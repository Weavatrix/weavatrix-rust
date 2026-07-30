mod language_fixture;

use language_fixture::Fixture;
use weavatrix_rust::{Analyzer, EdgeKind, NodeKind};

#[test]
fn resolves_relative_imports_to_repository_files() {
    let fixture = Fixture::new();
    fixture.write(
        "web/client.ts",
        "import { helper } from \"./helper\";\nexport function run(){ helper(); }\n",
    );
    fixture.write("web/helper.ts", "export function helper() {}\n");
    let snapshot = Analyzer::default().analyze(&fixture.root).unwrap();
    assert!(snapshot.edges.iter().any(|edge| {
        edge.kind == EdgeKind::Imports
            && edge.source.as_str() == "file:web/client.ts"
            && edge.target.as_str() == "file:web/helper.ts"
    }));
}

#[test]
fn resolves_imports_inside_nested_python_and_plain_java_source_roots() {
    let fixture = Fixture::new();
    fixture.write(
        "apps/reporting/python/main.py",
        "from service import load_report\n",
    );
    fixture.write(
        "apps/reporting/python/service.py",
        "from utils import normalize\n\ndef load_report():\n    return normalize('x')\n",
    );
    fixture.write(
        "apps/reporting/python/utils.py",
        "def normalize(value):\n    return value\n",
    );
    fixture.write(
        "fixtures/java/src/api/UserReader.java",
        "package api;\nimport model.User;\npublic interface UserReader {}\n",
    );
    fixture.write(
        "fixtures/java/src/model/User.java",
        "package model;\npublic class User {}\n",
    );

    let snapshot = Analyzer::default().analyze(&fixture.root).unwrap();
    let import = |source: &str, target: &str| {
        snapshot.edges.iter().any(|edge| {
            edge.kind == EdgeKind::Imports
                && edge.source.as_str() == source
                && edge.target.as_str() == target
        })
    };
    for (source, target) in [
        (
            "file:apps/reporting/python/main.py",
            "file:apps/reporting/python/service.py",
        ),
        (
            "file:apps/reporting/python/service.py",
            "file:apps/reporting/python/utils.py",
        ),
        (
            "file:fixtures/java/src/api/UserReader.java",
            "file:fixtures/java/src/model/User.java",
        ),
    ] {
        assert!(
            import(source, target),
            "nested source-root import must resolve: {source} -> {target}"
        );
    }
}

#[test]
fn resolves_language_specific_repository_imports() {
    let fixture = language_specific_import_fixture();
    let snapshot = Analyzer::default().analyze(&fixture.root).unwrap();
    let import = |source: &str, target: &str| {
        snapshot.edges.iter().any(|edge| {
            edge.kind == EdgeKind::Imports
                && edge.source.as_str() == source
                && edge.target.as_str() == target
        })
    };
    if cfg!(feature = "lang-rust") {
        assert!(
            import(
                "file:adapters/esp/src/lib.rs",
                "file:adapters/esp/src/ble.rs"
            ),
            "rust crate:: import must resolve inside the containing crate"
        );
    }
    assert!(
        import("file:job.py", "file:pkg/errors.py"),
        "python absolute module import must resolve to the file"
    );
    assert!(
        import("file:job.py", "file:pkg/__init__.py"),
        "python package import must resolve to __init__"
    );
    assert!(
        import("file:flowspec/flowspec.go", "file:kafkareader/reader.go"),
        "go module-path import must resolve to package files"
    );
    assert!(
        import(
            "file:src/main/java/com/x/Service.java",
            "file:src/main/java/com/x/Helper.java"
        ),
        "java classpath import must resolve through the source root"
    );
    assert!(
        import("file:app.js", "file:services/util.js"),
        "commonjs require must resolve to the file"
    );

    let symbol = |label: &str, kind: &NodeKind| {
        snapshot
            .nodes
            .iter()
            .any(|node| node.label == label && node.kind == *kind)
    };
    assert!(
        symbol("PROTOCOL", &NodeKind::Constant),
        "go const block members are symbols"
    );
    assert!(
        symbol("helper", &NodeKind::Custom("field".to_owned())),
        "java fields are symbols"
    );
    assert!(
        !symbol("forEach", &NodeKind::Method),
        "call chains are not method declarations"
    );
    assert!(
        snapshot
            .edges
            .iter()
            .any(|edge| edge.kind == EdgeKind::Inherits && edge.target.as_str().contains("Base")),
        "python class bases produce inherits evidence"
    );
}

fn language_specific_import_fixture() -> Fixture {
    let fixture = Fixture::new();
    // Rust: crate/super/bare module paths across a workspace member.
    fixture.write("src/lib.rs", "mod wlan;\npub use wlan::scan;\n");
    fixture.write("src/wlan.rs", "pub fn scan() {}\n");
    fixture.write(
        "adapters/esp/src/lib.rs",
        "use crate::ble::Driver;\nmod ble;\n",
    );
    fixture.write(
        "adapters/esp/src/ble.rs",
        "use super::x::Y;\npub struct Driver;\n",
    );
    // Python: absolute local package import plus class inheritance.
    fixture.write("pkg/__init__.py", "\n");
    fixture.write(
        "pkg/errors.py",
        "class Base(Exception):\n    pass\nclass Derived(Base):\n    pass\n",
    );
    fixture.write("job.py", "import pkg.errors\nfrom pkg import errors\n");
    // Go: grouped imports through the repository module path plus const block.
    fixture.write(
        "kafkareader/reader.go",
        "package kafkareader\nfunc Read() {}\n",
    );
    let module = fixture
        .root
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap()
        .to_owned();
    fixture.write(
        "flowspec/flowspec.go",
        &format!(
            "package flowspec\nimport (\n\tkr \"edgehawk.com/{module}/kafkareader\"\n)\nconst (\n\tPROTOCOL = 1\n\tSRC_ADDR = 2\n)\nfunc parse() {{ kr.Read() }}\n"
        ),
    );
    // Java: classpath import, field, and no call-chain method false positive.
    fixture.write(
        "src/main/java/com/x/Helper.java",
        "package com.x;\npublic class Helper {}\n",
    );
    fixture.write(
        "src/main/java/com/x/Service.java",
        "package com.x;\nimport com.x.Helper;\nimport static com.x.Helper.help;\npublic class Service {\nprivate final Helper helper = null;\npublic void run() {\nitems.forEach(item -> {\n});\n}\n}\n",
    );
    // JavaScript: CommonJS require and multi-line import closers.
    fixture.write("services/util.js", "module.exports = {};\n");
    fixture.write(
        "app.js",
        "const util = require('./services/util');\nimport {\n  a,\n} from './services/util';\n",
    );
    fixture
}
