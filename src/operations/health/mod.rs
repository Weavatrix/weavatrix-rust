mod audit;
mod clones;
mod coverage;
#[path = "graph/cycles.rs"]
mod cycles;
mod dead_code;
mod debt;
#[path = "deps/dependencies.rs"]
mod dependencies;
mod entry_points;
mod hot_paths;
mod manifests;
pub(in crate::operations) mod paths;
mod project_identity;
pub(super) mod runtime;
#[path = "graph/scc.rs"]
mod scc;
#[path = "evidence/test_evidence.rs"]
mod test_evidence;

pub(super) use audit::audit;
pub(super) use clones::duplicates;
pub(super) use coverage::coverage;
pub(super) use cycles::runtime_dependency_cycles;
pub(super) use dead_code::dead_code;
pub(super) use hot_paths::hot_paths;
pub(super) use paths::{is_non_product, is_test_suite, path_is_visible};
