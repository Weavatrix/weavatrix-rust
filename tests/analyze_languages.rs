use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};
use weavatrix_rust::{Analyzer, EdgeKind, NodeKind};

#[test]
fn extracts_primary_and_optional_language_facts() {
    let fixture = Fixture::new();
    fixture.write(
        "go/server.go",
        "package server\nimport \"net/http\"\nfunc run() { http.HandleFunc(\"/ready\", ready) }\n",
    );
    fixture.write("c/main.c", "int helper(void) { return 1; }\n");
    fixture.write(
        "cpp/main.cpp",
        "#include <vector>\nint compute() { return helper(); }\n",
    );
    fixture.write("ops/run.sh", "function deploy() {\n  kubectl apply\n}\n");
    fixture.write(
        "db/query.sql",
        "CREATE TABLE users(id bigint);\nSELECT * FROM users;\nUPDATE users SET id=2;\n",
    );
    fixture.write(
        "deploy/app.yaml",
        "apiVersion: apps/v1\nkind: Deployment\nmetadata:\n  name: api\n",
    );
    fixture.write(
        "web/router.js",
        "export function list() { return db.collection(\"users\"); }\nrouter.get(\"/items\", list);\nconst routes = {\n'/mapped': {\nPOST: list,\n},\n};\n",
    );
    fixture.write(
        "web/client.ts",
        "export class Client {}\nfunction publish(){ topic(\"orders\"); }\n",
    );
    fixture.write(
        "automation/app.py",
        "from flask import Flask\n@app.get(\"/jobs\")\ndef jobs():\n    return run()\n",
    );
    fixture.write(
        "warehouse/Controller.java",
        "public class Controller {\n@GetMapping(\"/stock\") public void stock() {}\n}\n",
    );
    fixture.write(
        "service/Controller.cs",
        "public class Controller {\n[HttpGet(\"/health\")] public void Health() {}\n}\n",
    );

    let snapshot = Analyzer::default().analyze(&fixture.root).unwrap();
    for language in [
        "go",
        "c",
        "cpp",
        "bash",
        "sql",
        "kubernetes",
        "javascript",
        "typescript",
        "python",
        "java",
        "csharp",
    ] {
        assert!(
            snapshot
                .capabilities
                .iter()
                .any(|capability| capability.id == format!("lang:{language}")),
            "missing {language} capability"
        );
    }
    for (kind, label) in [
        (NodeKind::Endpoint, "ANY /ready"),
        (NodeKind::Endpoint, "GET /items"),
        (NodeKind::Endpoint, "POST /mapped"),
        (NodeKind::Endpoint, "GET /jobs"),
        (NodeKind::Endpoint, "GET /stock"),
        (NodeKind::Endpoint, "GET /health"),
        (NodeKind::Table, "users"),
        (NodeKind::Collection, "users"),
        (NodeKind::Topic, "orders"),
        (NodeKind::KubernetesResource, "Deployment/api"),
    ] {
        assert!(
            snapshot
                .nodes
                .iter()
                .any(|node| node.kind == kind && node.label == label),
            "missing {kind:?} {label}"
        );
    }
    assert!(
        snapshot
            .edges
            .iter()
            .any(|edge| edge.kind == EdgeKind::Writes)
    );
    assert!(
        snapshot
            .edges
            .iter()
            .any(|edge| edge.kind == EdgeKind::Deploys)
    );
    assert!(
        snapshot
            .edges
            .iter()
            .any(|edge| edge.kind == EdgeKind::Publishes)
    );
}

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
#[allow(clippy::too_many_lines)]
fn resolves_language_specific_repository_imports() {
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

#[test]
fn resolves_the_module_aliases_a_project_declares() {
    let fixture = Fixture::new();
    fixture.write(
        "tsconfig.json",
        r#"{
  // Comments and trailing commas are normal in this file.
  "compilerOptions": {
    "baseUrl": ".",
    "paths": {
      "@app/*": ["src/app/*"],
      "@shared": ["src/shared/index.ts"],
    },
  },
}"#,
    );
    fixture.write(
        "package.json",
        r##"{"name":"root","workspaces":["packages/*"],"imports":{"#config/*":"./src/config/*"}}"##,
    );
    fixture.write("src/app/service.ts", "export function serve() {}\n");
    fixture.write("src/shared/index.ts", "export const shared = 1;\n");
    fixture.write("src/config/db.ts", "export const url = '';\n");
    fixture.write("src/base/root.ts", "export const root = 1;\n");
    fixture.write("packages/ui/package.json", r#"{"name":"@acme/ui"}"#);
    fixture.write("packages/ui/index.ts", "export const Button = 1;\n");
    fixture.write(
        "src/entry.ts",
        "import { serve } from '@app/service';\nimport { shared } from '@shared';\nimport { url } from '#config/db';\nimport { Button } from '@acme/ui';\nimport { root } from 'src/base/root';\nimport { missing } from './nowhere';\nexport const use = [serve, shared, url, Button, root, missing];\n",
    );

    let snapshot = Analyzer::default().analyze(&fixture.root).unwrap();
    let imports = |target: &str| {
        snapshot.edges.iter().any(|edge| {
            edge.kind == EdgeKind::Imports
                && edge.source.as_str() == "file:src/entry.ts"
                && edge.target.as_str() == target
        })
    };
    for (target, form) in [
        ("file:src/app/service.ts", "tsconfig paths wildcard"),
        ("file:src/shared/index.ts", "tsconfig paths exact mapping"),
        ("file:src/config/db.ts", "package.json subpath import"),
        ("file:packages/ui/index.ts", "workspace package name"),
        ("file:src/base/root.ts", "tsconfig baseUrl"),
    ] {
        assert!(imports(target), "{form} must resolve to {target}");
    }
    assert!(
        !snapshot
            .nodes
            .iter()
            .any(|node| node.id.as_str().starts_with("package:") && node.label.starts_with('@')),
        "an aliased local import must not be recorded as an external package"
    );
    let unresolved = snapshot
        .diagnostics
        .iter()
        .filter(|diagnostic| diagnostic.code == "import.unresolved")
        .collect::<Vec<_>>();
    assert_eq!(
        unresolved.len(),
        1,
        "the one genuinely missing target is reported, got {unresolved:?}"
    );
    assert!(
        unresolved[0].message.contains("./nowhere"),
        "the diagnostic names the specifier, got {:?}",
        unresolved[0].message
    );
}

#[test]
fn resolves_imports_through_re_export_barrels() {
    let fixture = Fixture::new();
    fixture.write("src/shared/Button.tsx", "export function Button() {}\n");
    fixture.write("src/shared/Input.tsx", "export function Input() {}\n");
    fixture.write(
        "src/shared/index.ts",
        "export { Button } from './Button';\nexport * from './Input';\n",
    );
    fixture.write(
        "src/app/App.tsx",
        "import { Button, Input } from '../shared';\nexport function App() { return Button(); }\n",
    );
    let snapshot = Analyzer::default().analyze(&fixture.root).unwrap();
    let edge = |kind: EdgeKind, source: &str, target: &str| {
        snapshot.edges.iter().any(|item| {
            item.kind == kind && item.source.as_str() == source && item.target.as_str() == target
        })
    };
    assert!(
        edge(
            EdgeKind::ReExports,
            "file:src/shared/index.ts",
            "file:src/shared/Button.tsx"
        ),
        "named re-export is recorded as re-export evidence"
    );
    assert!(
        edge(
            EdgeKind::ReExports,
            "file:src/shared/index.ts",
            "file:src/shared/Input.tsx"
        ),
        "star re-export is recorded as re-export evidence"
    );
    assert!(
        edge(
            EdgeKind::Imports,
            "file:src/app/App.tsx",
            "file:src/shared/index.ts"
        ),
        "the barrel itself stays an import target"
    );
    for defining in ["file:src/shared/Button.tsx", "file:src/shared/Input.tsx"] {
        assert!(
            edge(EdgeKind::Imports, "file:src/app/App.tsx", defining),
            "barrel import must reach {defining} through the re-export chain"
        );
    }
}

#[test]
fn resolves_express_mount_chains_to_full_paths() {
    let fixture = Fixture::new();
    fixture.write(
        "services/users/router.js",
        "const express = require('express');\nconst router = express.Router();\nrouter.get('/', list);\nrouter.get('/:id', read);\nmodule.exports = router;\n",
    );
    fixture.write(
        "services/api.js",
        "const usersRouter = require('./users/router');\nconst api = require('express').Router();\napi.use('/users', usersRouter);\nmodule.exports = api;\n",
    );
    fixture.write(
        "app.js",
        "const api = require('./services/api');\napp.use('/api', api);\n",
    );
    let snapshot = Analyzer::default().analyze(&fixture.root).unwrap();
    for label in ["GET /api/users", "GET /api/users/:id"] {
        assert!(
            snapshot
                .nodes
                .iter()
                .any(|node| node.kind == NodeKind::Endpoint && node.label == label),
            "missing mounted endpoint {label}"
        );
    }
    assert!(
        snapshot
            .nodes
            .iter()
            .any(|node| node.kind == NodeKind::Endpoint && node.label == "GET /"),
        "locally declared endpoint evidence is preserved"
    );
}

struct Fixture {
    root: PathBuf,
}

impl Fixture {
    fn new() -> Self {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "weavatrix-languages-{}-{nonce}",
            std::process::id()
        ));
        fs::create_dir_all(&root).unwrap();
        Self { root }
    }

    fn write(&self, relative: &str, contents: &str) {
        let path = self.root.join(relative);
        fs::create_dir_all(path.parent().unwrap_or(Path::new("."))).unwrap();
        fs::write(path, contents).unwrap();
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}
