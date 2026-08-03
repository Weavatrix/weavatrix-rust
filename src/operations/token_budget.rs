//! Deterministic token budgets for bounded answers.
//!
//! An agent knows its remaining context window in tokens, not in items or
//! bytes. `token_budget` lets it ask for "whatever fits" and receive an
//! honest account of what was dropped to fit.

use blazingly_json::{Value, json};

/// Four bytes of serialized JSON per token: a deterministic offline estimate,
/// deliberately conservative for ASCII-dominated source text.
const BYTES_PER_TOKEN: usize = 4;

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
                "fit": estimated <= budget
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
