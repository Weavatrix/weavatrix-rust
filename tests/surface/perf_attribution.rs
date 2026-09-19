//! A measurement series is external evidence. The engine correlates it with
//! the declarations that changed between the revisions it names, states the
//! harness noise it can observe, and never presents co-occurrence as a
//! profiler result.

#![cfg(feature = "git")]

use crate::support::GitFixture;
use blazingly_json::json;
use std::process::Command;
use weavatrix_rust::{Weavatrix, tools};

const ALPHA_BASE: &str = "export function alpha(rows) {\n  let total = 0;\n  \
     for (const row of rows) {\n    total += row;\n  }\n  return total;\n}\n";
// Same declaration line, same extent, different body: graph identity alone
// reports nothing here, which is exactly the change an experiment makes.
const ALPHA_TUNED: &str = "export function alpha(rows) {\n  let total = 0;\n  \
     for (const row of rows) {\n    total = total + row * 2;\n  }\n  return total;\n}\n";
const BETA_BASE: &str = "export function beta(rows) {\n  return rows.length;\n}\n";
const BETA_SLOWER: &str =
    "export function beta(rows) {\n  return rows.filter(Boolean).length;\n}\n";

fn head(fixture: &GitFixture) -> String {
    let output = Command::new("git")
        .args(["rev-parse", "HEAD"])
        .current_dir(&fixture.root)
        .output()
        .unwrap();
    assert!(output.status.success());
    String::from_utf8(output.stdout).unwrap().trim().to_owned()
}

/// Three revisions, each touching exactly one declaration, and the table a
/// harness would have appended to as it went.
fn measured_repository() -> GitFixture {
    let fixture = GitFixture::new();
    fixture.write("src/alpha.js", ALPHA_BASE);
    fixture.write("src/beta.js", BETA_BASE);
    fixture.commit("baseline");
    let first = head(&fixture);

    fixture.write("src/alpha.js", ALPHA_TUNED);
    fixture.commit("alpha tightened");
    let second = head(&fixture);

    fixture.write("src/beta.js", BETA_SLOWER);
    fixture.commit("beta regressed");
    let third = head(&fixture);

    fixture.write(
        "results.tsv",
        &format!(
            "commit\tNS_PER_OP\tnote\n\
             # harness restarted here\n\
             {first}\t1000\tbaseline\n\
             {second}\t800\talpha tightened\n\
             {second}\t805\trerun of the same build\n\
             {third}\t1200\tbeta regressed\n\
             badc0ffee\t900\tnot in this repository\n\
             {third}\t-\trun crashed\n"
        ),
    );
    fixture
}

#[test]
fn attributes_a_series_to_the_declarations_that_changed_in_the_same_step() {
    let fixture = measured_repository();
    let mut engine = Weavatrix::open(&fixture.root).unwrap();
    let report = tools::call(
        &mut engine,
        "perf_attribution",
        json!({"measurements_file": "results.tsv", "metric": "NS_PER_OP"}),
    )
    .unwrap();

    let series = &report["series"];
    assert_eq!(
        series["data_rows"], 6,
        "the comment row is not data: {series}"
    );
    assert_eq!(
        series["measurements_used"], 4,
        "a blank metric and an absent revision are not measurements: {series}"
    );
    assert_eq!(
        series["skipped_rows"], 2,
        "both unusable rows are reported, not dropped: {series}"
    );
    assert_eq!(report["revisions_analyzed"], 3);

    let steps = report["steps"].as_array().unwrap();
    assert_eq!(steps.len(), 3, "{report}");

    assert_eq!(steps[0]["verdict"], "improved");
    assert_eq!(steps[0]["delta"], -200.0);
    assert_eq!(steps[0]["delta_percent"], -20.0);
    assert_eq!(
        steps[0]["symbols"][0]["label"], "alpha",
        "a body rewrite that moves no span is still a change: {}",
        steps[0]
    );
    assert_eq!(steps[0]["symbols"][0]["change"], "modified");
    assert_eq!(steps[0]["changed_symbols"], 1);

    assert_eq!(steps[1]["kind"], "same_revision");
    assert_eq!(steps[1]["changed_symbols"], 0);

    assert_eq!(steps[2]["verdict"], "regressed");
    assert_eq!(steps[2]["delta"], 395.0);
    assert_eq!(steps[2]["delta_percent"], 49.0683);
    assert_eq!(steps[2]["symbols"][0]["label"], "beta");

    let by_symbol = report["by_symbol"].as_array().unwrap();
    assert_eq!(by_symbol.len(), 2, "{report}");
    assert_eq!(by_symbol[0]["label"], "beta");
    assert_eq!(by_symbol[0]["weighted_delta"], 395.0);
    assert_eq!(by_symbol[0]["co_changed_at_best"], 1);
    assert_eq!(by_symbol[0]["evidence"], "isolated");
    assert_eq!(by_symbol[1]["label"], "alpha");
    assert_eq!(by_symbol[1]["delta_sum"], -200.0);

    let noise = &report["noise_floor"];
    assert_eq!(noise["measured"], true, "{noise}");
    assert_eq!(noise["samples"], 1);
    assert_eq!(
        noise["max_abs_percent"], 0.625,
        "a repeated revision measures the harness, not the code: {noise}"
    );
}

#[test]
fn a_noise_band_reclassifies_the_steps_inside_it() {
    let fixture = measured_repository();
    let mut engine = Weavatrix::open(&fixture.root).unwrap();
    let report = tools::call(
        &mut engine,
        "perf_attribution",
        json!({
            "measurements_file": "results.tsv",
            "metric": "NS_PER_OP",
            "min_delta_percent": 25
        }),
    )
    .unwrap();

    let verdicts = report["steps"]
        .as_array()
        .unwrap()
        .iter()
        .map(|step| step["verdict"].as_str().unwrap())
        .collect::<Vec<_>>();
    assert_eq!(
        verdicts,
        vec!["flat", "flat", "regressed"],
        "only the step outside the band survives as a result: {report}"
    );
}

#[test]
fn the_direction_decides_which_sign_is_an_improvement() {
    let fixture = measured_repository();
    let mut engine = Weavatrix::open(&fixture.root).unwrap();
    let report = tools::call(
        &mut engine,
        "perf_attribution",
        json!({
            "measurements_file": "results.tsv",
            "metric": "NS_PER_OP",
            "direction": "higher_is_better"
        }),
    )
    .unwrap();

    assert_eq!(report["direction"], "higher_is_better");
    assert_eq!(report["steps"][0]["verdict"], "regressed");
    assert_eq!(report["steps"][2]["verdict"], "improved");
}

#[test]
fn a_path_scope_limits_attribution_to_the_declarations_inside_it() {
    let fixture = measured_repository();
    let mut engine = Weavatrix::open(&fixture.root).unwrap();
    let report = tools::call(
        &mut engine,
        "perf_attribution",
        json!({
            "measurements_file": "results.tsv",
            "metric": "NS_PER_OP",
            "path": "src/beta.js"
        }),
    )
    .unwrap();

    let labels = report["by_symbol"]
        .as_array()
        .unwrap()
        .iter()
        .map(|entry| entry["label"].as_str().unwrap())
        .collect::<Vec<_>>();
    assert_eq!(labels, vec!["beta"], "{report}");
    assert_eq!(
        report["steps"][0]["changed_symbols"], 0,
        "the alpha step changed nothing inside the scope: {report}"
    );
}

#[test]
fn unusable_inputs_are_caller_errors_that_name_what_was_available() {
    let fixture = measured_repository();
    let mut engine = Weavatrix::open(&fixture.root).unwrap();

    let error = tools::call(
        &mut engine,
        "perf_attribution",
        json!({"measurements_file": "results.tsv", "metric": "WALL_CLOCK"}),
    )
    .unwrap_err();
    assert!(
        error.contains("available columns") && error.contains("NS_PER_OP"),
        "a missing column names the ones that exist: {error}"
    );

    let error = tools::call(
        &mut engine,
        "perf_attribution",
        json!({
            "measurements_file": "results.tsv",
            "metric": "NS_PER_OP",
            "direction": "sideways"
        }),
    )
    .unwrap_err();
    assert!(error.contains("lower_is_better"), "{error}");

    let error = tools::call(
        &mut engine,
        "perf_attribution",
        json!({"measurements_file": "../elsewhere.tsv", "metric": "NS_PER_OP"}),
    )
    .unwrap_err();
    assert!(
        error.contains("escapes repository"),
        "a table outside the repository is refused, not read: {error}"
    );
}
