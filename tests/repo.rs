#![allow(dead_code)]

#[path = "language_fixture/mod.rs"]
mod language_fixture;
#[path = "mermaid_support/mod.rs"]
mod mermaid_support;
#[path = "support/mod.rs"]
mod support;
#[path = "tool_fixture/mod.rs"]
mod tool_fixture;
#[path = "web3_support/mod.rs"]
mod web3_support;

#[path = "repo/duplicate_evidence.rs"]
mod duplicate_evidence;
#[path = "repo/duplicate_visibility.rs"]
mod duplicate_visibility;
#[path = "repo/git_graph_diff.rs"]
mod git_graph_diff;
#[path = "repo/git_graph_diff_detail.rs"]
mod git_graph_diff_detail;
#[path = "repo/git_read_blob.rs"]
mod git_read_blob;
#[path = "repo/health_cycles.rs"]
mod health_cycles;
#[path = "repo/health_nested_cargo.rs"]
mod health_nested_cargo;
#[path = "repo/mermaid_flowcharts.rs"]
mod mermaid_flowcharts;
#[path = "repo/web3_consumers.rs"]
mod web3_consumers;
#[path = "repo/web3_integration.rs"]
mod web3_integration;
