//! Correlates a caller-supplied measurement series with the structural change
//! between the revisions that produced it.
//!
//! The engine measures nothing itself. A harness supplies revisions and the
//! numbers it recorded at them; this operation states which declarations
//! differ between the revisions of each step. That is co-occurrence between an
//! external measurement and static structural change, not profiler
//! attribution, and every report says so.

#[cfg(feature = "git")]
mod attribute;
#[cfg(feature = "git")]
mod rollup;
#[cfg(feature = "git")]
mod series;
#[cfg(feature = "git")]
mod sources;
#[cfg(feature = "git")]
mod steps;

#[cfg(feature = "git")]
pub(in crate::operations) use attribute::attribution;

#[cfg(not(feature = "git"))]
pub(in crate::operations) fn attribution(
    _state: &crate::engine::RepositoryState,
    _args: &blazingly_json::Value,
) -> Result<blazingly_json::Value, String> {
    Err("git capability is not compiled".to_owned())
}

/// Rounds to four decimals so the same series always renders the same bytes.
#[cfg(feature = "git")]
fn round(value: f64) -> f64 {
    (value * 10_000.0).round() / 10_000.0
}

/// Graph-sized counts always fit. The saturating fallback keeps the divisor
/// finite instead of introducing a cast that could silently wrap.
#[cfg(feature = "git")]
fn count_as_f64(count: usize) -> f64 {
    f64::from(u32::try_from(count).unwrap_or(u32::MAX))
}
