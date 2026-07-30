#![forbid(unsafe_code)]
#![doc = include_str!("../README.md")]

mod analyzer;
mod engine;
pub mod language;
mod model;
pub mod operations;

/// Backward-compatible name for the operation API.
pub use operations as tools;

pub use analyzer::{Analyzer, AnalyzerConfig, SourceInput};
pub use engine::{RepositoryState, Weavatrix};
pub use language::Language;
pub use model::{
    Capability, CapabilityState, Diagnostic, Error, Result, SNAPSHOT_SCHEMA_VERSION, Snapshot,
};
pub use weavatrix_graph::{
    Confidence, Edge, EdgeKind, EvidenceKind, Graph, GraphBuilder, Node, NodeId, NodeKind,
    Provenance, SourcePosition, SourceSpan,
};
#[cfg(feature = "memory")]
pub use weavatrix_memory as memory;

/// Version of the evidence engine compiled into this library.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
