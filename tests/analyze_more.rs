#![allow(dead_code)]

#[path = "language_fixture/mod.rs"]
mod language_fixture;
#[path = "support/mod.rs"]
mod support;

#[path = "analyze_more/analyze_import_resolution.rs"]
mod analyze_import_resolution;
#[path = "analyze_more/analyze_module_resolution.rs"]
mod analyze_module_resolution;
#[path = "analyze_more/analyze_python_call_resolution.rs"]
mod analyze_python_call_resolution;
#[path = "analyze_more/analyze_rust_call_scope.rs"]
mod analyze_rust_call_scope;
#[path = "analyze_more/analyze_rust_inline_modules.rs"]
mod analyze_rust_inline_modules;
#[path = "analyze_more/analyze_rust_symbol_references.rs"]
mod analyze_rust_symbol_references;
#[path = "analyze_more/analyze_rust_type_references.rs"]
mod analyze_rust_type_references;
#[path = "analyze_more/analyze_rust_visibility.rs"]
mod analyze_rust_visibility;
#[path = "analyze_more/build_topology.rs"]
mod build_topology;
#[path = "analyze_more/di_graph.rs"]
mod di_graph;
