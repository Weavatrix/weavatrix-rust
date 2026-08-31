//! Deterministic token budgets for bounded answers.
//!
//! An agent knows its remaining context window in tokens, not in items or
//! bytes. `token_budget` lets it ask for "whatever fits" and receive an
//! honest account of what was dropped to fit.

use blazingly_json::{Value, json};

/// Four bytes of serialized JSON per token: a deterministic offline estimate,
/// deliberately conservative for ASCII-dominated source text.
const BYTES_PER_TOKEN: usize = 4;

/// Operations that trim their answer to `token_budget` and report what they
/// dropped. The catalog offers the argument to exactly these, so the list
/// follows the compiled capabilities rather than naming an absent operation.
pub(crate) fn honoured() -> &'static [&'static str] {
    match (cfg!(feature = "search"), cfg!(feature = "git")) {
        (true, true) => &[
            "context_bundle",
            "git_history",
            "git_read_blob",
            "graph_diff",
            "query_graph",
            "read_source",
            "search_code",
        ],
        (true, false) => &[
            "context_bundle",
            "query_graph",
            "read_source",
            "search_code",
        ],
        (false, true) => &[
            "context_bundle",
            "git_history",
            "git_read_blob",
            "graph_diff",
            "query_graph",
            "read_source",
        ],
        (false, false) => &["context_bundle", "query_graph", "read_source"],
    }
}

/// Records that an operation could not apply the requested budget.
///
/// A caller sets `token_budget` to protect its context window, so a budget
/// that was not applied has to be visible in the answer. It is reported, never
/// refused: these operations are read-only and lossless, a caller reads the
/// same `token_budget` block from every operation, and an argument a tool
/// cannot use is not a reason to withhold the evidence it was asked for.
///
/// # Errors
///
/// Returns a caller error for a malformed or zero budget.
pub(crate) fn annotate_unapplied(
    tool: &str,
    args: &Value,
    report: &mut Value,
) -> Result<(), String> {
    let Some(budget) = requested(args)? else {
        return Ok(());
    };
    if honoured().contains(&tool) {
        return Ok(());
    }
    let estimated = estimate(report);
    if let Some(object) = report.as_object_mut() {
        object.insert(
            "token_budget".to_owned(),
            json!({
                "requested": budget,
                "estimated_tokens": estimated,
                "estimator": "serialized bytes / 4",
                "dropped_items": 0,
                "fit": estimated <= budget,
                "applied": false,
                "applied_by": honoured()
            }),
        );
    }
    Ok(())
}

pub(crate) fn requested(args: &Value) -> Result<Option<usize>, String> {
    let Some(budget) = super::optional_u64(args, "token_budget")? else {
        return Ok(None);
    };
    if budget == 0 {
        return Err("token_budget must be a positive integer".to_owned());
    }
    Ok(Some(usize::try_from(budget).unwrap_or(usize::MAX)))
}

pub(crate) fn estimate(value: &Value) -> usize {
    blazingly_json::to_vec(value).map_or(0, |bytes| bytes.len().div_ceil(BYTES_PER_TOKEN))
}

/// Trims the arrays named by `pointers`, in order, from the tail until the
/// whole report fits the budget, then records the outcome under
/// `token_budget`. Without a requested budget the report is returned intact
/// and unannotated.
pub(crate) fn fit(report: &mut Value, budget: Option<usize>, pointers: &[&str]) {
    let Some(budget) = budget else {
        return;
    };
    let mut dropped = 0usize;
    for pointer in pointers {
        dropped += fit_array(report, budget, pointer);
    }
    let estimated = estimate(report);
    if let Some(object) = report.as_object_mut() {
        object.insert(
            "token_budget".to_owned(),
            json!({
                "requested": budget,
                "estimated_tokens": estimated,
                "estimator": "serialized bytes / 4",
                "dropped_items": dropped,
                "fit": estimated <= budget,
                "applied": true
            }),
        );
    }
}

/// Halving keeps the number of full re-estimates logarithmic in the array
/// length; the final single-item steps make the cut close to minimal.
fn fit_array(report: &mut Value, budget: usize, pointer: &str) -> usize {
    let mut dropped = 0usize;
    while estimate(report) > budget {
        let Some(items) = report.pointer_mut(pointer).and_then(Value::as_array_mut) else {
            break;
        };
        if items.is_empty() {
            break;
        }
        let step = if items.len() > 32 { items.len() / 2 } else { 1 };
        items.truncate(items.len() - step);
        dropped += step;
    }
    dropped
}

#[cfg(test)]
mod tests {
    use super::annotate_unapplied;
    use blazingly_json::json;

    #[test]
    fn an_unapplied_budget_is_recorded_and_the_answer_is_kept() {
        let budgeted = json!({"label": "value", "token_budget": 800});
        let mut report = json!({"node": "value"});
        annotate_unapplied("inspect_symbol", &budgeted, &mut report).unwrap();
        assert_eq!(report["token_budget"]["applied"], false);
        assert_eq!(report["node"], "value");

        let mut applying = json!({"lines": []});
        annotate_unapplied("read_source", &budgeted, &mut applying).unwrap();
        assert!(
            applying.get("token_budget").is_none(),
            "an operation that applies the budget reports it itself"
        );

        let mut unbudgeted = json!({"node": "value"});
        annotate_unapplied("inspect_symbol", &json!({}), &mut unbudgeted).unwrap();
        assert!(unbudgeted.get("token_budget").is_none());
    }
}
