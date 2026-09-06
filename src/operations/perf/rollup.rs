//! Co-occurrence rollup from measured steps to declarations.

use super::steps::Symbol;
use super::{count_as_f64, round};
use blazingly_json::{Value, json};
use std::collections::BTreeMap;

struct Accumulator {
    label: String,
    kind: String,
    file: Option<String>,
    steps: usize,
    delta_sum: f64,
    weighted_delta: f64,
    best_isolation: usize,
    improved: usize,
    regressed: usize,
    flat: usize,
}

/// Accumulates every step a declaration changed in.
pub(super) struct Rollup {
    entries: BTreeMap<String, Accumulator>,
}

impl Rollup {
    pub(super) fn new() -> Self {
        Self {
            entries: BTreeMap::new(),
        }
    }

    /// Records one measured step against every declaration that changed in it.
    ///
    /// A step that moved forty declarations is weak evidence for each of them,
    /// so the credited share is the step delta divided by that count. The raw
    /// sum is kept beside it, because the share alone would hide a declaration
    /// that is present in every large step.
    pub(super) fn record(&mut self, symbols: &[Symbol], delta: f64, verdict: &str) {
        if symbols.is_empty() {
            return;
        }
        let share = delta / count_as_f64(symbols.len());
        for symbol in symbols {
            let entry = self
                .entries
                .entry(symbol.id.clone())
                .or_insert_with(|| Accumulator {
                    label: symbol.label.clone(),
                    kind: symbol.kind.clone(),
                    file: symbol.file.clone(),
                    steps: 0,
                    delta_sum: 0.0,
                    weighted_delta: 0.0,
                    best_isolation: usize::MAX,
                    improved: 0,
                    regressed: 0,
                    flat: 0,
                });
            entry.steps += 1;
            entry.delta_sum += delta;
            entry.weighted_delta += share;
            entry.best_isolation = entry.best_isolation.min(symbols.len());
            match verdict {
                "improved" => entry.improved += 1,
                "regressed" => entry.regressed += 1,
                _ => entry.flat += 1,
            }
        }
    }

    /// The strongest attributions first, ranked by credited magnitude and then
    /// by identity so the same series always renders the same report.
    pub(super) fn into_values(self, top: usize) -> Vec<Value> {
        let mut ranked = self.entries.into_iter().collect::<Vec<_>>();
        ranked.sort_by(|left, right| {
            right
                .1
                .weighted_delta
                .abs()
                .total_cmp(&left.1.weighted_delta.abs())
                .then_with(|| left.0.cmp(&right.0))
        });
        ranked
            .into_iter()
            .take(top)
            .map(|(id, entry)| {
                json!({
                    "id": id,
                    "label": entry.label,
                    "kind": entry.kind,
                    "file": entry.file,
                    "steps": entry.steps,
                    "delta_sum": round(entry.delta_sum),
                    "weighted_delta": round(entry.weighted_delta),
                    "co_changed_at_best": entry.best_isolation,
                    "evidence": if entry.best_isolation <= 1 {
                        "isolated"
                    } else {
                        "co_changed"
                    },
                    "verdicts": {
                        "improved": entry.improved,
                        "regressed": entry.regressed,
                        "flat": entry.flat
                    }
                })
            })
            .collect()
    }
}
