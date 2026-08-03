mod language_fixture;

use blazingly_json::json;
use language_fixture::Fixture;
use weavatrix_rust::{Weavatrix, tools};

fn long_module() -> String {
    use std::fmt::Write;
    (1..=200).fold(String::new(), |mut module, index| {
        let _ = writeln!(module, "export const value{index} = {index};");
        module
    })
}

#[test]
fn read_source_trims_lines_to_the_requested_token_budget() {
    let fixture = Fixture::new();
    fixture.write("src/big.js", &long_module());
    let mut engine = Weavatrix::open(&fixture.root).unwrap();

    let unbudgeted = tools::call(
        &mut engine,
        "read_source",
        json!({"path": "src/big.js", "after": 150}),
    )
    .unwrap();
    assert!(
        unbudgeted.get("token_budget").is_none(),
        "without a budget the contract is unchanged"
    );
    let full_lines = unbudgeted["lines"].as_array().unwrap().len();

    let budgeted = tools::call(
        &mut engine,
        "read_source",
        json!({"path": "src/big.js", "after": 150, "token_budget": 300}),
    )
    .unwrap();
    let kept = budgeted["lines"].as_array().unwrap().len();
    assert!(
        kept < full_lines,
        "the budget must trim: {kept} vs {full_lines}"
    );
    assert!(kept > 0, "a 300-token budget holds some lines");
    assert_eq!(budgeted["token_budget"]["fit"], true);
    assert!(budgeted["token_budget"]["dropped_items"].as_u64().unwrap() > 0);
    assert_eq!(
        budgeted["end_line"],
        budgeted["lines"].as_array().unwrap().last().unwrap()["line"],
        "end_line reflects the trimmed answer"
    );

    assert!(
        tools::call(
            &mut engine,
            "read_source",
            json!({"path": "src/big.js", "token_budget": 0}),
        )
        .unwrap_err()
        .contains("token_budget"),
        "a zero budget is a caller error"
    );
}

#[test]
#[cfg(feature = "search")]
fn search_code_reports_budget_truncation_honestly() {
    let fixture = Fixture::new();
    fixture.write("src/big.js", &long_module());
    let mut engine = Weavatrix::open(&fixture.root).unwrap();

    let budgeted = tools::call(
        &mut engine,
        "search_code",
        json!({"query": "export const", "max_results": 100, "token_budget": 200}),
    )
    .unwrap();
    let kept = budgeted["matches"].as_array().unwrap().len();
    assert!(kept < 100, "the budget must trim the match list: {kept}");
    assert_eq!(budgeted["returned_matches"], kept as u64);
    assert_eq!(budgeted["totals"]["returned_matches"], kept as u64);
    assert_eq!(budgeted["truncated"], true);
    assert_eq!(budgeted["token_budget"]["fit"], true);
}

#[test]
fn context_bundle_and_query_graph_fit_their_budgets() {
    let fixture = Fixture::new();
    fixture.write("src/big.js", &long_module());
    fixture.write(
        "src/hub.js",
        "import { value1 } from './big.js';\nexport function hub(){ return value1; }\n",
    );
    let mut engine = Weavatrix::open(&fixture.root).unwrap();

    let bundle = tools::call(
        &mut engine,
        "context_bundle",
        json!({"label": "hub", "token_budget": 400}),
    )
    .unwrap();
    assert_eq!(bundle["token_budget"]["requested"], 400);
    assert!(bundle["token_budget"]["estimated_tokens"].as_u64().unwrap() <= 400);

    let graph = tools::call(
        &mut engine,
        "query_graph",
        json!({"seed_files": ["src/big.js"], "depth": 2, "max_nodes": 200, "token_budget": 250}),
    )
    .unwrap();
    assert!(graph["token_budget"]["estimated_tokens"].as_u64().unwrap() <= 250);
    assert_eq!(
        graph["truncated"], true,
        "dropping nodes or edges to fit a budget is truncation"
    );
}
