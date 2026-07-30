use super::model::TestEvidence;
use crate::operations::optional_bool;
use blazingly_json::{Value, json};
use std::collections::BTreeSet;

pub(super) fn build_test_evidence(impact: &Value, args: &Value) -> Result<TestEvidence, String> {
    let suggested = suggested_tests(impact);
    let requested = requested_tests(args)?;
    if optional_bool(args, "run_tests")?.unwrap_or(false) {
        return Err(
            "run_tests=true is invalid for the process-free verified_change tool; execute tests externally and supply their evidence"
                .to_owned(),
        );
    }
    let reason = if requested.is_empty() {
        "no test command was requested"
    } else {
        "verified_change is process-free; execute the requested tests externally and attach their results"
    };
    let value = json!({
        "state": "COMPLETE",
        "requested": requested,
        "suggested_files": suggested,
        "execution": {
            "present": false,
            "reason": reason
        }
    });
    Ok(TestEvidence {
        suggested,
        requested,
        value,
    })
}

fn requested_tests(args: &Value) -> Result<Vec<String>, String> {
    args.get("tests")
        .map(|value| {
            value
                .as_array()
                .ok_or_else(|| "tests must be an array of strings".to_owned())?
                .iter()
                .map(|item| {
                    item.as_str()
                        .map(str::to_owned)
                        .ok_or_else(|| "tests must contain only strings".to_owned())
                })
                .collect()
        })
        .transpose()
        .map(Option::unwrap_or_default)
}

fn suggested_tests(impact: &Value) -> Vec<String> {
    impact["impacted_nodes"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|node| {
            node["span"]["file"]
                .as_str()
                .or_else(|| node["source_file"].as_str())
        })
        .filter(|path| {
            let lower = path.to_ascii_lowercase();
            lower.contains("/test")
                || lower.contains("\\test")
                || lower.contains(".test.")
                || lower.contains(".spec.")
                || lower.ends_with("_test.go")
        })
        .map(str::to_owned)
        .collect::<BTreeSet<_>>()
        .into_iter()
        .take(30)
        .collect()
}
