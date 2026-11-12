use crate::language_fixture::Fixture as LanguageFixture;
use crate::tool_fixture::Fixture;
use blazingly_json::{Value, json};
use weavatrix_rust::{Weavatrix, operations, tools};

const ALPHA: &str = "export function parse(value) { return value; }\n";
const BETA: &str = "export function parse(value) { return value + 1; }\n";
const APP: &str =
    "import { parse } from './alpha.js';\nexport function run() { return parse(1); }\n";

#[test]
fn module_map_is_production_first_unless_non_product_is_requested() {
    let fixture = Fixture::new();
    fixture.write("src/main.js", "export function main() { return 1; }\n");
    fixture.write(
        "test/main.test.js",
        "export function testMain() { return 1; }\n",
    );
    fixture.write("docs/guide.md", "# Guide\n");
    let mut engine = Weavatrix::open(&fixture.root).unwrap();

    let production = tools::call(&mut engine, "module_map", json!({})).unwrap();
    let production_paths = production["modules"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|module| module["path"].as_str())
        .collect::<Vec<_>>();
    assert_eq!(production_paths, ["src"]);

    let all = tools::call(
        &mut engine,
        "module_map",
        json!({"include_non_product": true}),
    )
    .unwrap();
    let all_paths = all["modules"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|module| module["path"].as_str())
        .collect::<Vec<_>>();
    assert!(all_paths.contains(&"src"));
    assert!(all_paths.contains(&"test"));
    assert!(all_paths.contains(&"docs"));

    assert!(
        tools::call(
            &mut engine,
            "module_map",
            json!({"include_non_product": "yes"}),
        )
        .is_err()
    );
    assert!(tools::call(&mut engine, "module_map", json!({"top_n": "many"})).is_err());
}

#[test]
fn an_ambiguous_label_returns_candidates_instead_of_failing() {
    let fixture = same_name_repo();
    let mut engine = Weavatrix::open(&fixture.root).unwrap();
    for tool in ["find_references", "go_to_definition"] {
        let report = call(&mut engine, tool, json!({"label": "parse"}));
        assert_eq!(report["state"], "AMBIGUOUS", "{tool}: {report}");
        let files = text_at(&report["candidates"], &["span", "file"]);
        assert!(
            files.contains(&"src/alpha.js") && files.contains(&"src/beta.js"),
            "{tool}"
        );
    }
    let resolved = call(&mut engine, "go_to_definition", json!({"label": "run"}));
    assert_eq!(resolved["state"], "RESOLVED");
    assert_eq!(resolved["definition"]["span"]["file"], "src/app.js");
    let id = call(&mut engine, "go_to_definition", json!({"label": "parse"}))["candidates"]
        .as_array()
        .unwrap()
        .iter()
        .find(|item| item["span"]["file"] == "src/alpha.js")
        .unwrap()["id"]
        .as_str()
        .unwrap()
        .to_owned();
    for tool in ["go_to_definition", "find_references"] {
        let chosen = call(&mut engine, tool, json!({"label": id}));
        assert_eq!(chosen["state"], "RESOLVED", "{tool}: {chosen}");
        assert_eq!(chosen["definition"]["span"]["file"], "src/alpha.js");
    }
    let missing =
        operations::call(&mut engine, "find_references", json!({"label": "nope"})).unwrap_err();
    assert!(missing.contains("node not found"), "{missing}");
    assert_eq!(
        call(&mut engine, "go_to_definition", json!({"label": "nope"}))["state"],
        "UNRESOLVED"
    );
}

#[test]
fn dependents_start_from_a_file_and_a_child_repository() {
    let fixture = same_name_repo();
    let mut engine = Weavatrix::open(&fixture.root).unwrap();
    let from_file = call(
        &mut engine,
        "get_dependents",
        json!({"path": "src/alpha.js"}),
    );
    assert!(
        text_at(&from_file["dependents"], &["node", "label"]).contains(&"run"),
        "{from_file}"
    );
    let narrowed = call(
        &mut engine,
        "get_dependents",
        json!({"label": "parse", "path": "src/alpha.js"}),
    );
    let files = seed_files(&narrowed);
    assert!(
        !files.is_empty() && files.iter().all(|file| *file == "src/alpha.js"),
        "{narrowed}"
    );
    let ambiguous =
        operations::call(&mut engine, "get_dependents", json!({"label": "parse"})).unwrap_err();
    assert!(ambiguous.contains("ambiguous"), "{ambiguous}");
    let bare = operations::call(&mut engine, "get_dependents", json!({})).unwrap_err();
    assert!(bare.contains("label or path"), "{bare}");

    let parent = LanguageFixture::new();
    parent.write(
        "app/parent/route.ts",
        "export function GET() { return 1; }\n",
    );
    parent.write(
        "shop/app/only/route.ts",
        "export function GET() { return 2; }\n",
    );
    parent.write("shop/src/alpha.js", ALPHA);
    parent.write("shop/src/beta.js", BETA);
    parent.write("shop/src/app.js", APP);
    let mut engine = Weavatrix::open(&parent.root).unwrap();
    let pin = engine.state().root().to_path_buf();
    let listed = call(&mut engine, "list_endpoints", json!({"repository": "shop"}));
    let labels = text_at(&listed["endpoints"], &["label"]);
    assert!(
        labels.contains(&"GET /only") && !labels.contains(&"GET /parent"),
        "{listed}"
    );
    let child = parent.root.join("shop");
    assert_eq!(
        canon(&listed["repository_context"]["root"]),
        canon_path(&child)
    );
    assert_eq!(engine.state().root(), pin.as_path());
    let stats = call(&mut engine, "graph_stats", json!({"repository": "shop"}));
    assert_eq!(
        canon(&stats["repository_context"]["root"]),
        canon_path(&child)
    );
    assert_eq!(engine.state().root(), pin.as_path());
    let dependents = call(
        &mut engine,
        "get_dependents",
        json!({"repository": "shop", "label": "parse", "path": "src/alpha.js"}),
    );
    assert!(
        text_at(&dependents["dependents"], &["node", "label"]).contains(&"run"),
        "{dependents}"
    );
    assert_eq!(
        canon(&dependents["repository_context"]["root"]),
        canon_path(&child)
    );
    assert_eq!(engine.state().root(), pin.as_path());
    let outside = std::env::temp_dir().join(format!("weavatrix-outside-{}", std::process::id()));
    std::fs::create_dir_all(&outside).unwrap();
    let escaped = operations::call(
        &mut engine,
        "list_endpoints",
        json!({"repository": outside.display().to_string()}),
    );
    let _ = std::fs::remove_dir_all(&outside);
    assert!(escaped.unwrap_err().contains("launch root"));
    assert_eq!(engine.state().root(), pin.as_path());
}

fn same_name_repo() -> LanguageFixture {
    let fixture = LanguageFixture::new();
    fixture.write("src/alpha.js", ALPHA);
    fixture.write("src/beta.js", BETA);
    fixture.write("src/app.js", APP);
    fixture
}

fn call(engine: &mut Weavatrix, name: &str, args: Value) -> Value {
    operations::call(engine, name, args).unwrap_or_else(|error| panic!("{name}: {error}"))
}

fn text_at<'a>(value: &'a Value, path: &[&str]) -> Vec<&'a str> {
    value
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|item| {
            path.iter()
                .try_fold(item, |current, key| current.get(*key))
                .and_then(Value::as_str)
        })
        .collect()
}

fn seed_files(report: &Value) -> Vec<&str> {
    let mut files = Vec::new();
    if let Some(file) = report["seed"]["span"]["file"].as_str() {
        files.push(file);
    }
    files.extend(text_at(&report["seeds"], &["span", "file"]));
    files
}

fn canon(value: &Value) -> std::path::PathBuf {
    canon_path(std::path::Path::new(value.as_str().unwrap_or_default()))
}

fn canon_path(path: &std::path::Path) -> std::path::PathBuf {
    path.canonicalize().unwrap_or_else(|_| path.to_path_buf())
}
