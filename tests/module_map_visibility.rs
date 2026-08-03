mod tool_fixture;

use blazingly_json::json;
use tool_fixture::Fixture;
use weavatrix_rust::{Weavatrix, tools};

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
