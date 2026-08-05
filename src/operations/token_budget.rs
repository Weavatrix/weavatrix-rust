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
#[cfg(feature = "search")]
const HONOURED: &[&str] = &[
    "context_bundle",
    "query_graph",
    "read_source",
    "search_code",
];
#[cfg(not(feature = "search"))]
const HONOURED: &[&str] = &["context_bundle", "query_graph", "read_source"];

/// Rejects a budget the named operation would silently ignore.
///
/// A caller sets `token_budget` to protect its context window. Accepting the
/// argument and returning an unbounded answer spends the window it was meant
/// to defend and leaves nothing to attribute the overrun to, so an operation
/// that cannot honour the budget has to say so.
///
/// # Errors
///
/// Returns the operations that do honour a budget.
pub(crate) fn reject_unsupported(tool: &str, args: &Value) -> Result<(), String> {
    if args.get("token_budget").is_none() || HONOURED.contains(&tool) {
        return Ok(());
    }
    Err(format!(
        "{tool} does not bound its answer by token_budget; it is honoured by {}",
        HONOURED.join(", ")
    ))
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

#[cfg(test)]
mod tests {
    use super::reject_unsupported;
    use blazingly_json::json;

    #[test]
    fn a_budget_is_rejected_by_an_operation_that_cannot_honour_it() {
        let budgeted = json!({"label": "value", "token_budget": 800});
        assert!(reject_unsupported("inspect_symbol", &budgeted).is_err());
        assert!(reject_unsupported("read_source", &budgeted).is_ok());
        assert!(reject_unsupported("inspect_symbol", &json!({"label": "value"})).is_ok());
    }
}
