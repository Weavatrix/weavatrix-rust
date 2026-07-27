mod imports;
mod references;
mod state;
mod support;

use crate::Result;
use crate::language::LanguageRegistry;
use crate::snapshot::Snapshot;
use state::AnalysisState;
use std::collections::BTreeSet;
use std::path::Path;
use support::{canonical_repository, capabilities};
use weavatrix_scan::{ScanOptions, ScanReport, Scanner};

#[derive(Debug, Clone)]
pub struct AnalyzerConfig {
    pub max_file_bytes: u64,
}

impl Default for AnalyzerConfig {
    fn default() -> Self {
        Self {
            max_file_bytes: 1_500_000,
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

    pub(crate) fn analyze_with_report(
        &self,
        repository: impl AsRef<Path>,
    ) -> Result<(Snapshot, ScanReport)> {
        let repository = canonical_repository(repository.as_ref())?;
        let scan = self.scan(&repository, None)?;
        let snapshot = self.analyze_report(&repository, &scan)?;
        Ok((snapshot, scan))
    }

    pub(crate) fn scan(
        &self,
        repository: &Path,
        previous: Option<&ScanReport>,
    ) -> Result<ScanReport> {
        let options = self.scan_options();
        let scanner = Scanner::new(repository).options(options);
        Ok(match previous {
            Some(previous) => scanner.scan_incremental(previous)?,
            None => scanner.scan()?,
        })
    }

    pub(crate) fn analyze_report(&self, repository: &Path, scan: &ScanReport) -> Result<Snapshot> {
        let mut state = AnalysisState::new(repository)?;
        state.add_scan_warnings(scan.warnings.clone());
        for file in &scan.files {
            state.process_file(file, &self.languages)?;
        }
        state.resolve_references()?;
        state.into_snapshot(
            repository,
            scan.revision.clone(),
            capabilities(&self.languages),
        )
    }

    fn scan_options(&self) -> ScanOptions {
        let extensions = self
            .languages
            .extensions()
            .map(str::to_owned)
            .collect::<BTreeSet<_>>();
        let mut options = ScanOptions::default().with_extensions(extensions);
        options.max_file_bytes = self.config.max_file_bytes;
        options
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
        let mut state = AnalysisState::new(&repository)?;
        for source in sources {
            if u64::try_from(source.bytes.len()).unwrap_or(u64::MAX) > self.config.max_file_bytes {
                continue;
            }
            state.process_source(
                &source.path,
                &source.bytes,
                source.content_hash.as_deref(),
                &self.languages,
            )?;
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
            Ok(serde_json::to_string_pretty(&snapshot)?)
        } else {
            Ok(serde_json::to_string(&snapshot)?)
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
