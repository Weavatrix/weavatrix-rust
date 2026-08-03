mod language_fixture;

use language_fixture::Fixture;
use weavatrix_rust::{Analyzer, EdgeKind};

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
fn resolves_javascript_modules_whose_stems_contain_dots() {
    let fixture = Fixture::new();
    fixture.write(
        "jsconfig.json",
        r##"{
  "compilerOptions": {
    "baseUrl": ".",
    "paths": {"#tests/*": ["tests/*"]}
  }
}"##,
    );
    fixture.write(
        "tests/common/common.actions.js",
        "export const action = true;\n",
    );
    fixture.write(
        "src/data/attacks.mock.data.js",
        "export const attacks = [];\n",
    );
    fixture.write("src/exact.module.js", "export const exact = true;\n");
    fixture.write(
        "src/entry.js",
        "import { action } from '#tests/common/common.actions';\n\
         import { attacks } from './data/attacks.mock.data';\n\
         import { exact } from './exact.module.js';\n\
         export const values = [action, attacks, exact];\n",
    );

    let snapshot = Analyzer::default().analyze(&fixture.root).unwrap();
    let targets = snapshot
        .edges
        .iter()
        .filter(|edge| {
            edge.kind == EdgeKind::Imports && edge.source.as_str() == "file:src/entry.js"
        })
        .map(|edge| edge.target.as_str())
        .collect::<Vec<_>>();

    for target in [
        "file:tests/common/common.actions.js",
        "file:src/data/attacks.mock.data.js",
        "file:src/exact.module.js",
    ] {
        assert!(
            targets.contains(&target),
            "dotted module stem must resolve to {target}, got {targets:?}"
        );
    }
    assert!(
        snapshot
            .diagnostics
            .iter()
            .all(|diagnostic| diagnostic.code != "import.unresolved"),
        "all dotted module imports should resolve, got {:?}",
        snapshot.diagnostics
    );
}

#[test]
fn resolves_typescript_runtime_extensions_without_overriding_real_javascript() {
    let fixture = Fixture::new();
    fixture.write(
        "src/entry.ts",
        "import './plain.js';\nimport './component.jsx';\nimport './module.js';\nimport './common.jsx';\nimport './exact.js';\n",
    );
    fixture.write("src/plain.ts", "export const plain = true;\n");
    fixture.write("src/component.tsx", "export const component = true;\n");
    fixture.write("src/module.mts", "export const moduleValue = true;\n");
    fixture.write("src/common.cts", "export const common = true;\n");
    fixture.write("src/exact.js", "export const runtime = true;\n");
    fixture.write("src/exact.ts", "export const source = true;\n");

    let snapshot = Analyzer::default().analyze(&fixture.root).unwrap();
    let imports = snapshot
        .edges
        .iter()
        .filter(|edge| {
            edge.kind == EdgeKind::Imports && edge.source.as_str() == "file:src/entry.ts"
        })
        .map(|edge| edge.target.as_str())
        .collect::<Vec<_>>();

    for target in [
        "file:src/plain.ts",
        "file:src/component.tsx",
        "file:src/module.mts",
        "file:src/common.cts",
        "file:src/exact.js",
    ] {
        assert!(
            imports.contains(&target),
            "runtime specifier must resolve to {target}, got {imports:?}"
        );
    }
    assert!(
        !imports.contains(&"file:src/exact.ts"),
        "an exact JavaScript target must win over its TypeScript sibling"
    );
    assert!(
        snapshot
            .diagnostics
            .iter()
            .all(|diagnostic| diagnostic.code != "import.unresolved"),
        "all runtime specifiers should resolve, got {:?}",
        snapshot.diagnostics
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
