#[cfg(feature = "clone")]
mod tool_fixture;

#[cfg(feature = "clone")]
use blazingly_json::json;
#[cfg(feature = "clone")]
use tool_fixture::Fixture;
#[cfg(feature = "clone")]
use weavatrix_rust::{Weavatrix, tools};

#[test]
#[cfg(feature = "clone")]
fn duplicate_filter_rebuilds_families_without_test_members_or_dangling_pairs() {
    let fixture = Fixture::new();
    let source = "\
export function transform(value) {
  if (value > 10) {
    return value * 2;
  }
  return value + 1;
}
";
    fixture.write("src/left.js", source);
    fixture.write("src/right.js", source);
    fixture.write("tests/vector_linker.js", source);
    let mut engine = Weavatrix::open(&fixture.root).unwrap();

    let report = tools::call(
        &mut engine,
        "find_duplicates",
        json!({
            "mode": "strict",
            "min_tokens": 12,
            "top_n": 100,
            "include_tests": false
        }),
    )
    .unwrap();
    let families = report["families"].as_array().unwrap();
    let pairs = report["pairs"].as_array().unwrap();
    let returned_pair_ids = pairs
        .iter()
        .map(|pair| pair["id"].as_str().unwrap())
        .collect::<std::collections::BTreeSet<_>>();

    assert_eq!(
        families.len(),
        1,
        "expected one production family: {report:?}"
    );
    assert_eq!(pairs.len(), 1, "expected one production pair: {report:?}");
    for family in families {
        let members = family["members"].as_array().unwrap();
        let family_pair_ids = family["pairs"].as_array().unwrap();
        assert!(members.len() >= 2, "empty family survived: {family:?}");
        assert!(
            members
                .iter()
                .all(|member| !member["path"].as_str().unwrap().starts_with("tests/")),
            "excluded test member survived: {family:?}"
        );
        assert!(
            !family_pair_ids.is_empty(),
            "family without a returned pair survived: {family:?}"
        );
        assert!(
            family_pair_ids
                .iter()
                .all(|id| returned_pair_ids.contains(id.as_str().unwrap())),
            "family contains a dangling pair id: {family:?}"
        );
    }

    let limited = tools::call(
        &mut engine,
        "find_duplicates",
        json!({
            "mode": "strict",
            "min_tokens": 12,
            "top_n": 1,
            "include_tests": true
        }),
    )
    .unwrap();
    let limited_families = limited["families"].as_array().unwrap();
    let limited_pairs = limited["pairs"].as_array().unwrap();
    assert_eq!(
        limited_families.len(),
        1,
        "top_n family mismatch: {limited:?}"
    );
    assert_eq!(limited_pairs.len(), 1, "top_n pair mismatch: {limited:?}");
    assert_eq!(
        limited_families[0]["pairs"][0], limited_pairs[0]["id"],
        "top_n left a dangling family pair id: {limited:?}"
    );
    assert_eq!(
        limited_families[0]["members"].as_array().unwrap().len(),
        2,
        "top_n family members were not rebuilt from the returned pair: {limited:?}"
    );
}

#[test]
#[cfg(feature = "clone")]
fn duplicate_filter_drops_a_family_without_a_production_pair() {
    let fixture = Fixture::new();
    let source = "\
export function transform(value) {
  if (value > 10) {
    return value * 2;
  }
  return value + 1;
}
";
    fixture.write("src/only.js", source);
    fixture.write("tests/left.js", source);
    fixture.write("tests/right.js", source);
    let mut engine = Weavatrix::open(&fixture.root).unwrap();

    let report = tools::call(
        &mut engine,
        "find_duplicates",
        json!({
            "mode": "strict",
            "min_tokens": 12,
            "top_n": 100,
            "include_tests": false
        }),
    )
    .unwrap();

    assert_eq!(
        report["families"],
        json!([]),
        "empty family survived: {report:?}"
    );
    assert_eq!(
        report["pairs"],
        json!([]),
        "excluded pair survived: {report:?}"
    );
}

#[test]
#[cfg(feature = "clone")]
fn model_schema_clones_remain_visible_in_high_recall_and_explicit_filtered_modes() {
    let fixture = Fixture::new();
    let schema = "\
export const archiveSchema = connection.createSchema({
  id: String,
  externalId: String,
  packetsPerSecond: Number,
  bitsPerSecond: Number,
  maximumPackets: Number,
  maximumBits: Number,
  severity: String,
  active: Boolean,
  protectedObjectId: String,
  attackId: String,
});
";
    fixture.write("models/detection.model.js", schema);
    fixture.write("contracts/detection.schema.js", schema);
    let mut engine = Weavatrix::open(&fixture.root).unwrap();

    for arguments in [
        json!({
            "mode": "strict",
            "min_tokens": 12,
            "top_n": 100
        }),
        json!({
            "mode": "strict",
            "min_tokens": 12,
            "top_n": 100,
            "include_declarative": false
        }),
    ] {
        let report = tools::call(&mut engine, "find_duplicates", arguments).unwrap();
        assert!(
            report["families"].as_array().is_some_and(|families| {
                families.iter().any(|family| {
                    let paths = family["members"]
                        .as_array()
                        .into_iter()
                        .flatten()
                        .filter_map(|member| member["path"].as_str())
                        .collect::<std::collections::BTreeSet<_>>();
                    paths.contains("models/detection.model.js")
                        && paths.contains("contracts/detection.schema.js")
                })
            }),
            "model/schema clone was hidden: {report:?}"
        );
    }
}
