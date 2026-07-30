use super::Value;

pub(super) const DEFAULT_MAX_FILES: usize = 5_000;
pub(super) const MAX_MAX_FILES: usize = 25_000;
pub(super) const DEFAULT_MAX_FILE_BYTES: u64 = 2 * 1_024 * 1_024;
pub(super) const MAX_MAX_FILE_BYTES: u64 = 16 * 1_024 * 1_024;
pub(super) const DEFAULT_MAX_OBSERVATIONS: usize = 5_000;
pub(super) const MAX_MAX_OBSERVATIONS: usize = 20_000;
pub(super) const DEFAULT_PER_CONTRACT: usize = 8;
pub(super) const MAX_PER_CONTRACT: usize = 50;
pub(super) const RUNTIME_SCHEMA: &str = "weavatrix.transport-runtime.v1";
pub(super) const DEFAULT_RUNTIME_REPORTS: &[&str] = &[
    ".weavatrix/transport-runtime.json",
    ".weavatrix/reports/transport-runtime.json",
];
pub(super) const MAX_RUNTIME_REPORT_BYTES: u64 = 2 * 1_024 * 1_024;
pub(super) const MAX_RUNTIME_OBSERVATIONS: usize = 10_000;

pub(super) fn bounded_usize(args: &Value, key: &str, default: usize, maximum: usize) -> usize {
    args.get(key)
        .and_then(Value::as_u64)
        .and_then(|value| usize::try_from(value).ok())
        .unwrap_or(default)
        .clamp(1, maximum)
}

pub(super) fn bounded_u64(args: &Value, key: &str, default: u64, maximum: u64) -> u64 {
    args.get(key)
        .and_then(Value::as_u64)
        .unwrap_or(default)
        .clamp(1, maximum)
}
