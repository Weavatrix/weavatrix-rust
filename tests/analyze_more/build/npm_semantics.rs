use crate::language_fixture::Fixture;
use blazingly_json::{Value, json};
use weavatrix_rust::{Weavatrix, tools};

fn npm_member(report: &Value) -> &Value {
    &report["workspaces"]
        .as_array()
        .unwrap()
        .iter()
        .find(|workspace| workspace["ecosystem"] == "npm")
        .unwrap()["members"][0]
}

#[test]
fn npm_script_graph_preserves_chains_cycles_and_argument_forwarding() {
    let fixture = Fixture::new();
    fixture.write(
        "package.json",
        r#"{
          "name": "scripts",
          "scripts": {
            "build": "npm run compile -- --pretty",
            "compile": "pnpm run build",
            "lint": "eslint .",
            "check": "yarn lint && npm run build"
          }
        }"#,
    );
    fixture.write("index.js", "export const value = 1;\n");
    let mut engine = Weavatrix::open(&fixture.root).unwrap();

    let report = tools::call(&mut engine, "build_graph", json!({})).unwrap();
    let tasks = npm_member(&report)["tasks"].as_array().unwrap();
    let task = |name: &str| {
        tasks
            .iter()
            .find(|task| task["name"] == name)
            .unwrap_or_else(|| panic!("missing {name}: {report}"))
    };
    assert_eq!(task("build")["cycle"], true);
    assert_eq!(task("compile")["cycle"], true);
    assert_eq!(task("build")["argument_forwarding"], "EXPLICIT_DOUBLE_DASH");
    assert_eq!(task("lint")["argument_forwarding"], "NOT_APPLICABLE");
    assert_eq!(task("check")["invokes"].as_array().unwrap().len(), 2);
    assert!(
        task("check")["invokes"]
            .as_array()
            .unwrap()
            .iter()
            .all(|id| id.as_str().is_some_and(|id| id.starts_with("task:")))
    );
}

#[test]
fn typescript_extends_options_and_shared_source_membership_are_preserved() {
    let fixture = Fixture::new();
    fixture.write("package.json", r#"{"name":"web"}"#);
    fixture.write(
        "tsconfig.base.json",
        r#"{"exclude":["**/*"],"compilerOptions":{"strict":true}}"#,
    );
    fixture.write(
        "tsconfig.json",
        r#"{"extends":"./tsconfig.base.json","include":["src/**/*.ts"],"compilerOptions":{"composite":true}}"#,
    );
    fixture.write(
        "tsconfig.test.json",
        r#"{"files":["src/shared.ts"],"compilerOptions":{"composite":false}}"#,
    );
    fixture.write("src/shared.ts", "export const value = 1;\n");
    let mut engine = Weavatrix::open(&fixture.root).unwrap();

    let build = tools::call(&mut engine, "build_graph", json!({})).unwrap();
    let member = npm_member(&build);
    let target = member["targets"]
        .as_array()
        .unwrap()
        .iter()
        .find(|target| target["name"] == "tsconfig.json")
        .unwrap();
    assert_eq!(target["options"]["composite"], true);
    assert_eq!(target["options"]["extends"], "./tsconfig.base.json");
    assert!(
        member["dependencies"]
            .as_array()
            .unwrap()
            .iter()
            .any(|dependency| dependency["scope"] == "tsconfig_extends"
                && dependency["resolution"] == "LOCAL_TARGET")
    );

    let prepared = tools::call(
        &mut engine,
        "prepare_change",
        json!({"files": ["src/shared.ts"]}),
    )
    .unwrap();
    let membership = &prepared["ci_protection"]["changed"][0]["build_membership"];
    assert_eq!(membership["resolution"], "TARGET_SOURCE_FALLBACK");
    assert_eq!(membership["target_ids"].as_array().unwrap().len(), 2);
}
