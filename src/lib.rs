#![forbid(unsafe_code)]
#![doc = include_str!("../README.md")]

mod analyzer;
mod engine;
mod error;
pub mod language;
pub mod mcp;
mod snapshot;
pub mod tools;

pub use analyzer::{Analyzer, AnalyzerConfig, SourceInput};
pub use engine::{RepositoryState, Weavatrix};
pub use error::{Error, Result};
pub use language::Language;
pub use snapshot::{Capability, CapabilityState, Diagnostic, SNAPSHOT_SCHEMA_VERSION, Snapshot};
pub use weavatrix_graph::{
    Confidence, Edge, EdgeKind, EvidenceKind, Graph, GraphBuilder, Node, NodeId, NodeKind,
    Provenance, SourcePosition, SourceSpan,
};
#[cfg(feature = "memory")]
pub use weavatrix_memory as memory;
