#![allow(dead_code)]

#[path = "language_fixture/mod.rs"]
mod language_fixture;
#[path = "support/mod.rs"]
mod support;

#[path = "surface/application_boundary.rs"]
mod application_boundary;
#[path = "surface/cli_identity.rs"]
mod cli_identity;
#[path = "surface/cli_machine_mode.rs"]
mod cli_machine_mode;
#[path = "surface/memory_boundary.rs"]
mod memory_boundary;
#[path = "surface/no_unknown_states.rs"]
mod no_unknown_states;
#[path = "surface/perf_attribution.rs"]
mod perf_attribution;
#[path = "surface/report_composition.rs"]
mod report_composition;
#[path = "surface/token_budgets.rs"]
mod token_budgets;
