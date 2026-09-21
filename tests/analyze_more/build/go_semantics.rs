use crate::language_fixture::Fixture;
use blazingly_json::{Value, json};
use weavatrix_rust::{Weavatrix, tools};

fn go_workspace(report: &Value) -> &Value {
    report["workspaces"]
        .as_array()
        .unwrap()
        .iter()
        .find(|workspace| workspace["ecosystem"] == "go")
        .unwrap_or_else(|| panic!("missing Go workspace: {report}"))
}

#[test]
fn go_imports_link_packages_across_workspace_modules_and_keep_external_frontier() {
    let fixture = Fixture::new();
    fixture.write("go.work", "go 1.25\nuse (\n  ./a\n  ./b\n)\n");
    fixture.write("a/go.mod", "module example.test/a\n\ngo 1.25\n");
    fixture.write(
        "a/app.go",
        "package app\nimport (\n  \"example.test/b/pkg\"\n  \"net/http\"\n)\n",
    );
    fixture.write("b/go.mod", "module example.test/b\n\ngo 1.25\n");
    fixture.write("b/pkg/pkg.go", "package pkg\n");
    let mut engine = Weavatrix::open(&fixture.root).unwrap();

    let report = tools::call(&mut engine, "build_graph", json!({})).unwrap();
    let workspace = go_workspace(&report);
    let app = workspace["members"]
        .as_array()
        .unwrap()
        .iter()
        .find(|member| member["name"] == "example.test/a")
        .unwrap();
    let dependencies = app["dependencies"].as_array().unwrap();
    let local = dependencies
        .iter()
        .find(|dependency| dependency["name"] == "example.test/b/pkg")
        .unwrap();
    assert_eq!(local["resolution"], "LOCAL_WORKSPACE_MODULE");
    assert!(local["member"].as_str().is_some());
    assert!(local["source_target"].as_str().is_some());
    assert!(local["target"].as_str().is_some());
    let external = dependencies
        .iter()
        .find(|dependency| dependency["name"] == "net/http")
        .unwrap();
    assert_eq!(external["resolution"], "EXTERNAL_IMPORT");
    assert!(external["target"].is_null());
    assert_eq!(app["internal_dependencies"].as_array().unwrap().len(), 1);
}

#[test]
fn conditional_go_import_keeps_its_source_predicate() {
    let fixture = Fixture::new();
    fixture.write("go.mod", "module example.test/app\n\ngo 1.25\n");
    fixture.write(
        "platform_windows.go",
        "//go:build windows\npackage app\nimport \"syscall\"\n",
    );
    let mut engine = Weavatrix::open(&fixture.root).unwrap();

    let report = tools::call(&mut engine, "build_graph", json!({})).unwrap();
    let dependency = &go_workspace(&report)["members"][0]["dependencies"][0];
    assert_eq!(dependency["resolution"], "EXTERNAL_IMPORT");
    assert!(
        dependency["condition"]
            .as_str()
            .unwrap()
            .contains("windows")
    );
    assert_eq!(dependency["condition_ast"]["op"], "all");
}
