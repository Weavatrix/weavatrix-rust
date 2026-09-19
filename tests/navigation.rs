#![allow(dead_code)]

#[path = "language_fixture/mod.rs"]
mod language_fixture;
#[path = "support/mod.rs"]
mod support;
#[path = "tool_fixture/mod.rs"]
mod tool_fixture;

#[path = "navigation/context_bundle_budget.rs"]
mod context_bundle_budget;
#[path = "navigation/http_contract_linker.rs"]
mod http_contract_linker;
#[path = "navigation/map_stacktrace.rs"]
mod map_stacktrace;
#[path = "navigation/module_map_visibility.rs"]
mod module_map_visibility;
#[path = "navigation/occurrence_navigation.rs"]
mod occurrence_navigation;
#[path = "navigation/occurrence_scip.rs"]
mod occurrence_scip;
#[path = "navigation/read_source_extent.rs"]
mod read_source_extent;
#[path = "navigation/select_tests_selection.rs"]
mod select_tests_selection;
#[path = "navigation/trace_endpoint.rs"]
mod trace_endpoint;
#[path = "navigation/typed_contract_trace.rs"]
mod typed_contract_trace;
