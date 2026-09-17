//! Admission and decode budgets. Checked before allocations.

pub(crate) const MAX_ABI_BYTES: u64 = 4 * 1024 * 1024;
pub(crate) const MAX_ARTIFACT_BYTES: u64 = 16 * 1024 * 1024;
pub(crate) const MAX_BUILD_INFO_BYTES: u64 = 64 * 1024 * 1024;
pub(crate) const MAX_NESTING: usize = 64;
pub(crate) const MAX_MEMBERS: usize = 10_000;
pub(crate) const MAX_SOURCES: usize = 10_000;
#[allow(dead_code)]
pub(crate) const DEFAULT_FILE_BYTES: u64 = MAX_ABI_BYTES;
