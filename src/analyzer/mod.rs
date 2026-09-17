//! Repository scan, extraction, resolution, and snapshot orchestration.

mod imports;
mod mounts;
mod pipeline;
mod references;
mod state;
mod support;

use crate::language::LanguageRegistry;
use crate::model::{Result, Snapshot};
use pipeline::parse_parallel;
use state::{AnalysisState, parse_source};
use std::path::Path;
use support::{canonical_repository, capabilities};

#[derive(Debug, Clone)]
pub struct AnalyzerConfig {
    pub max_file_bytes: u64,
    /// n8n JSON and Dify YAML exports can exceed the general file cap; oversized non-export files
    /// still stops at `max_file_bytes`.
    pub n8n_max_file_bytes: u64,
    /// Foundry/solc build-info can exceed the n8n/Dify cap; other JSON still stops earlier.
    pub web3_max_file_bytes: u64,
}

impl Default for AnalyzerConfig {
    fn default() -> Self {
        Self {
            max_file_bytes: crate::language::N8N_DEFAULT_FILE_BYTES,
            n8n_max_file_bytes: 16 * 1024 * 1024,
            web3_max_file_bytes: crate::language::WEB3_MAX_BUILD_INFO_BYTES,
        }
    }
}

pub struct Analyzer {
    config: AnalyzerConfig,
    languages: LanguageRegistry,
}

#[derive(Debug, Clone)]
pub struct SourceInput {
    pub path: String,
    pub bytes: Vec<u8>,
    pub content_hash: Option<String>,
}

impl Default for Analyzer {
    fn default() -> Self {
        Self::new(AnalyzerConfig::default())
    }
}

impl Analyzer {
    #[must_use]
    pub fn new(config: AnalyzerConfig) -> Self {
        Self {
            config,
            languages: LanguageRegistry::default(),
        }
    }

    #[must_use]
    pub fn supports_path(&self, path: &str) -> bool {
        Path::new(path)
            .extension()
            .and_then(|value| value.to_str())
            .map(str::to_ascii_lowercase)
            .is_some_and(|extension| self.languages.adapter_for_extension(&extension).is_some())
    }

    #[must_use]
    pub const fn max_file_bytes(&self) -> u64 {
        self.config.max_file_bytes
    }

    /// Analyzes a repository into a deterministic, evidence-carrying snapshot.
    ///
    /// # Errors
    ///
    /// Returns an error when the repository cannot be read, an adapter cannot
    /// initialize, or normalized facts violate graph integrity.
    pub fn analyze(&self, repository: impl AsRef<Path>) -> Result<Snapshot> {
        self.analyze_with_report(repository)
            .map(|(snapshot, _)| snapshot)
    }

    /// Analyzes an immutable set of source blobs without materializing a tree.
    ///
    /// This is the bridge used by the Git module for revision-aware graph
    /// comparisons. The repository path only supplies stable repository
    /// identity; every analyzed byte comes from `sources`.
    ///
    /// # Errors
    ///
    /// Returns parser or graph validation failures.
    pub fn analyze_sources(
        &self,
        repository: impl AsRef<Path>,
        revision: impl Into<String>,
        sources: impl IntoIterator<Item = SourceInput>,
    ) -> Result<Snapshot> {
        let repository = canonical_repository(repository.as_ref())?;
        let sources = sources
            .into_iter()
            .filter(|source| admits_source(&source.path, &source.bytes, &self.config))
            .collect::<Vec<_>>();
        let mut parsed = parse_parallel(sources.len(), |index| {
            let source = &sources[index];
            parse_source(
                &source.path,
                &source.bytes,
                source.content_hash.as_deref(),
                &self.languages,
            )
        })?;
        mounts::apply(&mut parsed);
        let (node_hint, edge_hint) = AnalysisState::expected(&parsed);
        let mut state = AnalysisState::with_capacity(&repository, node_hint, edge_hint)?;
        for item in parsed {
            state.integrate(item)?;
        }
        state.resolve_references()?;
        state.into_snapshot(&repository, revision.into(), capabilities(&self.languages))
    }

    /// Analyzes a repository and serializes the snapshot as JSON.
    ///
    /// # Errors
    ///
    /// Returns any analysis error or a JSON serialization error.
    pub fn analyze_json(&self, repository: impl AsRef<Path>, pretty: bool) -> Result<String> {
        let snapshot = self.analyze(repository)?;
        if pretty {
            Ok(blazingly_json::to_string_pretty(&snapshot)?)
        } else {
            Ok(blazingly_json::to_string(&snapshot)?)
        }
    }

    /// Analyzes a repository and serializes a JS Weavatrix-compatible graph.
    ///
    /// # Errors
    ///
    /// Returns any analysis error or a JSON serialization error.
    pub fn analyze_legacy_json(
        &self,
        repository: impl AsRef<Path>,
        pretty: bool,
    ) -> Result<String> {
        Ok(self.analyze(repository)?.legacy_json(pretty)?)
    }
}

fn admits_source(path: &str, bytes: &[u8], config: &AnalyzerConfig) -> bool {
    let size = u64::try_from(bytes.len()).unwrap_or(u64::MAX);
    if size <= config.max_file_bytes {
        return true;
    }
    if size > config.n8n_max_file_bytes.max(config.web3_max_file_bytes) {
        return false;
    }
    let Ok(text) = std::str::from_utf8(bytes) else {
        return false;
    };
    let extension = Path::new(path)
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or("");
    if extension.eq_ignore_ascii_case("json") {
        crate::language::n8n_looks_promising(text)
            || crate::language::agent_looks_promising(path, text)
            || (crate::language::web3_looks_promising(text)
                && crate::language::web3_admitted_size(text, size)
                && size <= config.web3_max_file_bytes)
    } else if extension.eq_ignore_ascii_case("yaml") || extension.eq_ignore_ascii_case("yml") {
        crate::language::dify_looks_promising(text)
    } else if extension.eq_ignore_ascii_case("md") || extension.eq_ignore_ascii_case("mdx") {
        crate::language::agent_looks_promising(path, text)
            || crate::language::mermaid_looks_promising(path, text)
    } else if extension.eq_ignore_ascii_case("mmd") || extension.eq_ignore_ascii_case("mermaid") {
        crate::language::mermaid_looks_promising(path, text)
    } else {
        false
    }
}
