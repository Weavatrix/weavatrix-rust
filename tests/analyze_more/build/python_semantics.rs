use crate::language_fixture::Fixture;
use blazingly_json::{Value, json};
use weavatrix_rust::{Weavatrix, tools};

fn python_member(report: &Value) -> &Value {
    &report["workspaces"]
        .as_array()
        .unwrap()
        .iter()
        .find(|workspace| workspace["ecosystem"] == "python")
        .unwrap()["members"][0]
}

#[test]
fn pyproject_dependencies_local_frontier_and_pytest_config_are_distinct() {
    let fixture = Fixture::new();
    fixture.write(
        "pyproject.toml",
        r"[project]
name = 'python#dist'
version = '1.0'
dependencies = [
  'requests>=2',
  'shared @ file:../shared',
]

[project.optional-dependencies]
test = ['pytest>=9']

[project.scripts]
serve = 'app.cli:main'

[tool.pytest.ini_options]
testpaths = ['tests']
",
    );
    fixture.write("app/__init__.py", "VALUE = 1\n");
    let mut engine = Weavatrix::open(&fixture.root).unwrap();

    let report = tools::call(&mut engine, "build_graph", json!({})).unwrap();
    let member = python_member(&report);
    assert_eq!(member["name"], "python#dist");
    assert_eq!(member["dependencies"].as_array().unwrap().len(), 3);
    let local = member["dependencies"]
        .as_array()
        .unwrap()
        .iter()
        .find(|dependency| dependency["name"] == "shared")
        .unwrap();
    assert_eq!(local["resolution"], "LOCAL_PATH_UNRESOLVED");
    assert!(
        member["tasks"]
            .as_array()
            .unwrap()
            .iter()
            .any(|task| task["kind"] == "test_configuration" && task["name"] == "pytest")
    );
    assert!(
        member["tasks"]
            .as_array()
            .unwrap()
            .iter()
            .any(|task| task["kind"] == "entry_point" && task["name"] == "serve")
    );
}

#[test]
fn invalid_pyproject_is_reported_as_incomplete() {
    let fixture = Fixture::new();
    fixture.write("pyproject.toml", "[project\nname = 'broken'\n");
    let mut engine = Weavatrix::open(&fixture.root).unwrap();

    let report = tools::call(&mut engine, "build_graph", json!({})).unwrap();
    assert_eq!(report["status"], "INCOMPLETE");
    assert!(
        report["manifest_diagnostics"]
            .as_array()
            .unwrap()
            .iter()
            .any(|diagnostic| diagnostic["manifest"] == "pyproject.toml"
                && diagnostic["reason"] == "invalid_pyproject_manifest")
    );
}
