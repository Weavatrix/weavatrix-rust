//! JSON schemas for the operation catalog.

mod descriptions;
mod field_types;
mod optional_sections;
mod tool_fields;

use optional_sections::{extension_fields, health_fields, occurrence_fields, perf_fields};
use tool_fields::{change_fields, graph_fields, source_and_api_fields};

pub(super) use field_types::field_schema;

pub(super) fn optional_fields(tool: &str) -> &'static [&'static str] {
    graph_fields(tool)
        .or_else(|| change_fields(tool))
        .or_else(|| occurrence_fields(tool))
        .or_else(|| source_and_api_fields(tool))
        .or_else(|| health_fields(tool))
        .or_else(|| perf_fields(tool))
        .or_else(|| extension_fields(tool))
        .unwrap_or(&[])
}
