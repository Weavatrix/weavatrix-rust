#![cfg(feature = "git")]

mod support;

use blazingly_json::json;
use support::GitFixture;
use weavatrix_rust::{Weavatrix, tools};

#[test]
fn graph_diff_rolls_edge_churn_up_to_file_pairs_by_default() {
    let fixture = GitFixture::new();
    fixture.write("src/lib.rs", "mod caller;\nmod target;\n");
    fixture.write(
        "src/caller.rs",
        "use crate::target::target;\npub fn caller() {}\n",
    );
    fixture.write("src/target.rs", "pub fn target() {}\n");
    fixture.commit("baseline");
    fixture.write(
        "src/caller.rs",
        "use crate::target::target;\npub fn caller() { target(); }\n",
    );

    let mut engine = Weavatrix::open(&fixture.root).unwrap();
    let compact = tools::call(&mut engine, "graph_diff", json!({"base_ref": "HEAD"})).unwrap();
    let pairs = compact["edges"]["by_file"].as_array().unwrap();

    assert_eq!(compact["edges"]["detail"], "file_pairs");
    assert!(
        pairs.iter().any(|pair| {
            pair["from"] == "src/caller.rs"
                && pair["to"] == "src/target.rs"
                && pair["relation"] == "calls"
                && pair["added"] == 1
                && pair["removed"] == 0
        }),
        "the compact diff must retain the actionable cross-file call change: {pairs:?}"
    );

    let detailed = tools::call(
        &mut engine,
        "graph_diff",
        json!({"base_ref": "HEAD", "detail": "edges"}),
    )
    .unwrap();
    assert_eq!(detailed["edges"]["detail"], "edges");
    assert!(detailed["edges"]["added"].as_array().is_some());
    assert!(detailed["edges"]["by_file"].is_null());
}
