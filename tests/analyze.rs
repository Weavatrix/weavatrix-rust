#![allow(dead_code)]

#[path = "language_fixture/mod.rs"]
mod language_fixture;
#[path = "support/mod.rs"]
mod support;
#[path = "tool_fixture/mod.rs"]
mod tool_fixture;

#[path = "analyze/analyze_contracts.rs"]
mod analyze_contracts;
#[path = "analyze/analyze_declaration_extents.rs"]
mod analyze_declaration_extents;
#[path = "analyze/analyze_http_routes.rs"]
mod analyze_http_routes;
#[path = "analyze/analyze_json.rs"]
mod analyze_json;
#[path = "analyze/analyze_language_capabilities.rs"]
mod analyze_language_capabilities;
#[path = "analyze/analyze_page_resources.rs"]
mod analyze_page_resources;
#[path = "analyze/analyze_reachability.rs"]
mod analyze_reachability;
#[path = "analyze/analyze_rust.rs"]
mod analyze_rust;
#[path = "analyze/analyze_scope_filters.rs"]
mod analyze_scope_filters;
#[path = "analyze/analyze_swift.rs"]
mod analyze_swift;
