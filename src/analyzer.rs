mod imports;
mod mounts;
mod references;
mod state;
mod support;

use crate::Result;
use crate::error::Error;
use crate::language::LanguageRegistry;
use crate::snapshot::Snapshot;
use state::{AnalysisState, ParsedSource, parse_source};
use std::collections::BTreeSet;
use std::path::Path;
use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};
use support::{canonical_repository, capabilities};
use weavatrix_scan::{ScanOptions, ScanReport, Scanner};

/// Runs `fetch` for every index across all available cores and returns the
/// results in index order, so downstream integration stays deterministic.
fn parse_parallel<F>(count: usize, fetch: F) -> Result<Vec<ParsedSource>>
where
    F: Fn(usize) -> Result<ParsedSource> + Sync,
{
    let workers = std::thread::available_parallelism()
        .map_or(1, usize::from)
        .min(count)
        .max(1);
    if workers <= 1 {
        return (0..count).map(fetch).collect();
    }
    let cursor = AtomicUsize::new(0);
    let results = Mutex::new(Vec::with_capacity(count));
    std::thread::scope(|scope| {
        for _ in 0..workers {
            scope.spawn(|| {
                loop {
                    let index = cursor.fetch_add(1, Ordering::Relaxed);
                    if index >= count {
                        break;
                    }
                    let result = fetch(index);
                    let mut guard = results.lock().expect("parser thread panicked");
                    guard.push((index, result));
                }
            });
        }
    });
    let mut items = results.into_inner().expect("parser thread panicked");
    items.sort_unstable_by_key(|(index, _)| *index);
    items.into_iter().map(|(_, result)| result).collect()
}

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
        let timing = std::env::var_os("WEAVATRIX_PHASE_TIMING").is_some();
        let started = std::time::Instant::now();
        let mut parsed = parse_parallel(scan.files.len(), |index| {
            let file = &scan.files[index];
            let bytes = std::fs::read(&file.absolute)
                .map_err(|source| Error::io(&file.absolute, source))?;
            parse_source(
                &file.relative,
                &bytes,
                file.content_hash.as_deref(),
                &self.languages,
            )
        })?;
        mounts::apply(&mut parsed);
        let parsed_at = started.elapsed();
        let (node_hint, edge_hint) = AnalysisState::expected(&parsed);
        let mut state = AnalysisState::with_capacity(repository, node_hint, edge_hint)?;
        state.add_scan_warnings(scan.warnings.clone());
        for item in parsed {
            state.integrate(item)?;
        }
        let integrated_at = started.elapsed();
        state.resolve_references()?;
        let resolved_at = started.elapsed();
        let snapshot = state.into_snapshot(
            repository,
            scan.revision.clone(),
            capabilities(&self.languages),
        );
        if timing {
            eprintln!(
                "phase-timing parse={:.1}ms integrate={:.1}ms resolve={:.1}ms snapshot={:.1}ms",
                parsed_at.as_secs_f64() * 1e3,
                integrated_at.saturating_sub(parsed_at).as_secs_f64() * 1e3,
                resolved_at.saturating_sub(integrated_at).as_secs_f64() * 1e3,
                started.elapsed().saturating_sub(resolved_at).as_secs_f64() * 1e3,
            );
        }
        snapshot
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
        let sources = sources
            .into_iter()
            .filter(|source| {
                u64::try_from(source.bytes.len()).unwrap_or(u64::MAX) <= self.config.max_file_bytes
            })
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
