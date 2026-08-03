mod language_fixture;

use blazingly_json::json;
use language_fixture::Fixture;
use weavatrix_rust::{Weavatrix, tools};

#[test]
#[cfg(feature = "git")]
fn changed_files_select_dependent_and_name_convention_suites() {
    let fixture = Fixture::new();
    fixture.write(
        "services/init.js",
        "export function init(){ return 1; }\n",
    );
    fixture.write(
        "services/consumer.js",
        "import { init } from './init.js';\nexport const start = () => init();\n",
    );
    fixture.write(
        "tests/init.test.js",
        "import { init } from '../services/init.js';\ntest('init', () => init());\n",
    );
    fixture.write(
        "tests/integration/consumer.spec.js",
        "import { start } from '../../services/consumer.js';\ntest('start', () => start());\n",
    );
    fixture.write(
        "tests/unrelated.test.js",
        "test('unrelated', () => 1);\n",
    );
    let mut engine = Weavatrix::open(&fixture.root).unwrap();

    let report = tools::call(
        &mut engine,
        "select_tests",
        json!({"files": ["services/init.js"], "depth": 3}),
    )
    .unwrap();
    let paths = report["tests"]
        .as_array()
        .unwrap()
        .iter()
        .map(|test| test["path"].as_str().unwrap().to_owned())
        .collect::<Vec<_>>();

    assert!(
        paths.contains(&"tests/init.test.js".to_owned()),
        "direct suite by dependency and name: {report:?}"
    );
    assert!(
        paths.contains(&"tests/integration/consumer.spec.js".to_owned()),
        "transitive dependent suite through consumer.js: {report:?}"
    );
    assert!(
        !paths.contains(&"tests/unrelated.test.js".to_owned()),
        "an unrelated suite is not selected: {report:?}"
    );
    assert_eq!(report["coverage_evidence"]["present"], false);
}

#[test]
#[cfg(feature = "git")]
fn changed_test_files_select_themselves_and_ranking_prefers_proximity() {
    let fixture = Fixture::new();
    fixture.write("src/lib.js", "export const value = 1;\n");
    fixture.write(
        "tests/lib.test.js",
        "import { value } from '../src/lib.js';\ntest('value', () => value);\n",
    );
    let mut engine = Weavatrix::open(&fixture.root).unwrap();

    let self_selected = tools::call(
        &mut engine,
        "select_tests",
        json!({"files": ["tests/lib.test.js"]}),
    )
    .unwrap();
    let tests = self_selected["tests"].as_array().unwrap();
    assert_eq!(tests.len(), 1);
    assert_eq!(tests[0]["path"], "tests/lib.test.js");
    assert_eq!(tests[0]["distance"], 0);
    assert_eq!(tests[0]["reasons"][0]["kind"], "changed_test");

    let ranked = tools::call(
        &mut engine,
        "select_tests",
        json!({"files": ["src/lib.js"]}),
    )
    .unwrap();
    let first = &ranked["tests"][0];
    assert_eq!(first["path"], "tests/lib.test.js");
    assert!(
        first["reasons"]
            .as_array()
            .unwrap()
            .iter()
            .any(|reason| reason["kind"] == "dependent"),
        "the suite is selected through the reverse dependency: {ranked:?}"
    );

    assert!(
        tools::call(
            &mut engine,
            "select_tests",
            json!({"files": ["src/lib.js"], "precision": "lsp"}),
        )
        .unwrap_err()
        .contains("precision"),
        "unsupported precision is an explicit error"
    );
}

#[test]
#[cfg(feature = "git")]
fn python_and_go_conventions_select_by_name() {
    let fixture = Fixture::new();
    fixture.write("app/report.py", "def build():\n    return 1\n");
    fixture.write(
        "tests/test_report.py",
        "from app.report import build\n\ndef test_build():\n    assert build() == 1\n",
    );
    fixture.write("pkg/parser.go", "package pkg\n\nfunc Parse() int { return 1 }\n");
    fixture.write(
        "pkg/parser_test.go",
        "package pkg\n\nimport \"testing\"\n\nfunc TestParse(t *testing.T) { Parse() }\n",
    );
    let mut engine = Weavatrix::open(&fixture.root).unwrap();

    let report = tools::call(
        &mut engine,
        "select_tests",
        json!({"files": ["app/report.py", "pkg/parser.go"]}),
    )
    .unwrap();
    let paths = report["tests"]
        .as_array()
        .unwrap()
        .iter()
        .map(|test| test["path"].as_str().unwrap().to_owned())
        .collect::<Vec<_>>();
    assert!(paths.contains(&"tests/test_report.py".to_owned()), "{report:?}");
    assert!(paths.contains(&"pkg/parser_test.go".to_owned()), "{report:?}");
}
