//! Lane refresh, idle unload, and the request-log clock.

use super::{Lane, Record};
use crate::analyzer::Analyzer;
use crate::model::captured::EvidenceSnapshot;
use blazingly_json::{Value, json};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard, TryLockError};
use std::time::{Duration, Instant};

const IDLE: Duration = Duration::from_secs(20 * 60);
const COALESCE: Duration = Duration::from_secs(1);

pub(super) fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

pub(super) fn fresh(at: Instant) -> bool {
    Instant::now().saturating_duration_since(at) < COALESCE
}

pub(super) fn text<'a>(args: &'a Value, key: &str) -> Option<&'a str> {
    let value = args.get(key)?.as_str()?.trim();
    (!value.is_empty()).then_some(value)
}

pub(super) fn clip(value: &str, max: usize) -> String {
    if value == "unset" || value == "pin" {
        return value.to_owned();
    }
    value.chars().take(max).collect()
}

pub(super) fn state_ready(lane: &Lane) -> bool {
    match lane.inner.try_lock() {
        Ok(inner) => inner.state.is_some(),
        Err(_) => false,
    }
}

pub(super) fn lane_loaded(lane: &Lane) -> bool {
    match lane.inner.try_lock() {
        Ok(inner) => inner.state.is_some(),
        Err(TryLockError::WouldBlock) => true,
        Err(TryLockError::Poisoned(poison)) => poison.into_inner().state.is_some(),
    }
}

pub(super) fn sweep(lanes: &mut BTreeMap<PathBuf, Arc<Lane>>, launch: &Path) {
    let now = Instant::now();
    lanes.retain(|path, lane| {
        if path == launch {
            return true;
        }
        match lane.inner.try_lock() {
            Ok(inner) => now.saturating_duration_since(inner.used) < IDLE,
            Err(TryLockError::WouldBlock) => true,
            Err(TryLockError::Poisoned(poison)) => {
                now.saturating_duration_since(poison.into_inner().used) < IDLE
            }
        }
    });
}

pub(super) fn refresh_lane(state: &mut crate::engine::RepositoryState) -> Result<bool, String> {
    let analyzer = Analyzer::default();
    let scan = analyzer
        .scan(&state.root, Some(state.scan.as_ref()))
        .map_err(|error| error.to_string())?;
    let evidence = EvidenceSnapshot::capture(&state.root, &scan);
    if scan.revision == state.scan.revision {
        state.scan = Arc::new(scan);
        state.evidence = Arc::new(evidence);
        return Ok(false);
    }
    *state = crate::engine::RepositoryState::from_scan(&analyzer, &state.root, scan)
        .map_err(|error| error.to_string())?;
    Ok(true)
}

pub(super) fn record_json(row: &Record) -> Value {
    json!({
        "agent": row.agent,
        "repository": row.repository,
        "tool": row.tool,
        "at": row.at,
        "at_utc": row.stamp,
        "reused": row.reused,
        "ok": row.ok,
        "elapsed_ms": row.elapsed_ms,
    })
}

pub(super) fn utc_stamp(seconds: u64) -> String {
    let days = i64::try_from(seconds / 86_400).unwrap_or(i64::MAX);
    let time = seconds % 86_400;
    let (year, month, day) = civil(days);
    format!(
        "{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}Z",
        time / 3_600,
        (time % 3_600) / 60,
        time % 60
    )
}

fn civil(days: i64) -> (i64, u32, u32) {
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = u64::try_from(z - era * 146_097).unwrap_or(0);
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = i64::try_from(yoe).unwrap_or(0) + era * 400 + i64::from(month <= 2);
    (
        year,
        u32::try_from(month).unwrap_or(0),
        u32::try_from(day).unwrap_or(0),
    )
}
