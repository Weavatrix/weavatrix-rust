mod web3_support;

use blazingly_json::json;
use weavatrix_rust::tools;
use web3_support::{dump, engine};

#[test]
fn deposit_indexed_mask_change_reaches_old_viem_consumers() {
    let (_fixture, mut engine) = engine();
    let inventory = tools::call(&mut engine, "web3_inventory", json!({})).unwrap();
    let text = dump(&inventory);
    assert!(text.contains("Deposit"), "{text}");
    assert!(
        text.contains("web3.consumer") || text.contains("decodeEventLog"),
        "{text}"
    );
    assert_eq!(inventory["coverage"]["deployment"], "not_provided");

    let events = engine
        .state()
        .graph()
        .nodes()
        .iter()
        .filter(|node| node.kind.as_str() == "web3.consumer")
        .count();
    assert!(
        events >= 2,
        "both proven decodeEventLog sites must remain: {events}"
    );

    let impact = tools::call(&mut engine, "web3_impact", json!({"task": "change-event"})).unwrap();
    let report = dump(&impact);
    assert!(report.contains("EVENT_LAYOUT_CHANGED"), "{report}");
    assert!(report.contains("silent_misdecode"), "{report}");
    assert!(
        report.contains("0x000000000000000000000000000000000000002a"),
        "{report}"
    );
    assert!(report.contains("not_provided"), "{report}");
    assert!(
        !report.contains("deployed implementation was verified"),
        "{report}"
    );
}

#[test]
fn same_selector_does_not_merge_two_contracts() {
    let (_fixture, engine) = engine();
    let transfers = engine
        .state()
        .graph()
        .nodes()
        .iter()
        .filter(|node| node.label == "transfer(address,uint256)")
        .count();
    assert_eq!(
        transfers, 2,
        "two contracts must keep separate transfer members"
    );
}

#[test]
fn output_change_is_kept_when_selector_matches() {
    let (_fixture, mut engine) = engine();
    let left = engine
        .state()
        .graph()
        .nodes()
        .iter()
        .find(|node| {
            node.label == "transfer(address,uint256)"
                && node
                    .span
                    .as_ref()
                    .is_some_and(|span| span.file.contains("Other.json"))
        })
        .unwrap()
        .id
        .clone();
    let context = tools::call(
        &mut engine,
        "web3_context",
        json!({"label": left.as_str(), "task": "change-output"}),
    )
    .unwrap();
    let text = dump(&context);
    assert!(
        text.contains("(bool)") || text.contains("web3.return"),
        "{text}"
    );
}

#[test]
fn unknown_spread_stays_unresolved() {
    let (_fixture, engine) = engine();
    let unresolved = engine.state().graph().nodes().iter().any(|node| {
        node.kind.as_str() == "web3.consumer"
            && engine
                .state()
                .graph()
                .nodes()
                .iter()
                .any(|domain| domain.label == "web3.unresolved:dynamic_or_spread")
    });
    assert!(unresolved, "spread after abi must not invent a target");
}

#[test]
fn package_manifest_is_not_an_abi() {
    let (_fixture, engine) = engine();
    assert!(
        engine
            .state()
            .graph()
            .nodes()
            .iter()
            .filter(|node| node.label == "web3-fixtures")
            .all(|node| node.kind.as_str() != "web3.abi")
    );
}
