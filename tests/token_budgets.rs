mod language_fixture;
#[allow(dead_code)]
mod support;

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

/// The operations that trim their answer to `token_budget`, per compiled
/// capability. Two of them answer from Git, so the list follows both features.
///
/// Pinned on both sides: the catalog must offer the argument to exactly these,
/// and every other operation must refuse it. Drift in either direction is the
/// defect this list exists to catch - a schema that promises a bound the code
/// does not apply.
fn honoured() -> Vec<&'static str> {
    let mut names = vec!["context_bundle", "query_graph", "read_source"];
    if cfg!(feature = "git") {
        names.extend(["git_history", "graph_diff"]);
    }
    if cfg!(feature = "search") {
        names.push("search_code");
    }
    names
}

#[test]
fn the_catalog_offers_a_budget_to_exactly_the_operations_that_apply_one() {
    use std::collections::BTreeSet;

    let declared = tools::catalog()
        .into_iter()
        .filter(|tool| {
            tool.input_schema["properties"]
                .get("token_budget")
                .is_some()
        })
        .map(|tool| tool.name)
        .collect::<BTreeSet<_>>();

    assert_eq!(
        declared,
        honoured().into_iter().collect(),
        "the declared budget surface drifted from the implemented one"
    );
}

/// Hotspots and co-change diff every commit and are most of the answer. A
/// caller reading recent history is not asking for them, and the cheap answer
/// must stay cheap.
#[cfg(feature = "git")]
#[test]
fn reading_recent_history_does_not_pay_for_the_analysis_it_did_not_ask_for() {
    let fixture = support::GitFixture::new();
    for index in 0..12 {
        fixture.write(
            "src/lib.rs",
            &format!("pub fn step() -> u32 {{ {index} }}\n"),
        );
        fixture.write(&format!("src/m{index}.rs"), "pub fn helper() {}\n");
        fixture.commit(&format!("step {index}"));
    }
    let mut engine = Weavatrix::open(&fixture.root).unwrap();

    let plain = tools::call(&mut engine, "git_history", json!({"max_commits": 12})).unwrap();
    let analysed = tools::call(
        &mut engine,
        "git_history",
        json!({"max_commits": 12, "include_analytics": true}),
    )
    .unwrap();

    assert_eq!(plain["analytics"]["present"], false);
    assert!(
        plain["commits"].as_array().is_some_and(|it| it.len() == 12),
        "the commit log itself is still the answer"
    );
    // The blocks that cost a diff per commit are the ones that must be absent.
    // How much that saves is repository-shaped, so the invariant is checked
    // here and the magnitude is measured on a real repository.
    for absent in ["hotspots", "cochange_pairs", "commits"] {
        assert!(
            plain["analytics"][absent].is_null(),
            "{absent} must not be computed unless it was asked for"
        );
    }
    assert!(analysed["analytics"]["hotspots"].is_array());
    assert!(
        estimate(&plain) < estimate(&analysed),
        "the default must be cheaper: {} vs {}",
        estimate(&plain),
        estimate(&analysed)
    );

    let bounded = tools::call(
        &mut engine,
        "git_history",
        json!({"max_commits": 12, "token_budget": 200}),
    )
    .unwrap();
    assert_eq!(bounded["token_budget"]["applied"], true);
    assert!(
        bounded["token_budget"]["dropped_items"]
            .as_u64()
            .unwrap_or(0)
            > 0
    );
}

#[cfg(feature = "git")]
fn estimate(value: &blazingly_json::Value) -> usize {
    blazingly_json::to_vec(value).map_or(0, |bytes| bytes.len().div_ceil(4))
}

#[test]
fn an_operation_that_cannot_apply_the_budget_answers_and_says_so() {
    let fixture = Fixture::new();
    fixture.write("src/big.js", &long_module());
    let mut engine = Weavatrix::open(&fixture.root).unwrap();

    // The answer is never withheld over an argument the operation cannot use;
    // a caller that set a budget to protect its context window learns from the
    // same block it reads everywhere else that nothing was trimmed.
    for tool in ["inspect_symbol", "get_dependents", "graph_stats"] {
        let report = tools::call(
            &mut engine,
            tool,
            json!({"label": "value1", "file": "src/big.js", "token_budget": 800}),
        )
        .unwrap_or_else(|error| panic!("{tool} withheld its answer over a budget: {error}"));

        assert_eq!(
            report["token_budget"]["applied"], false,
            "{tool} must record that it did not apply the budget: {report:?}"
        );
        assert_eq!(report["token_budget"]["requested"], 800);
        assert_eq!(report["token_budget"]["dropped_items"], 0);
        assert!(
            report["token_budget"]["estimated_tokens"]
                .as_u64()
                .is_some(),
            "{tool} must state what its answer costs: {report:?}"
        );
    }

    let applied = tools::call(
        &mut engine,
        "read_source",
        json!({"path": "src/big.js", "after": 150, "token_budget": 300}),
    )
    .unwrap();
    assert_eq!(
        applied["token_budget"]["applied"], true,
        "an operation that trims says so on the same field: {applied:?}"
    );
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
