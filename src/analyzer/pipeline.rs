use super::state::{AnalysisState, ParsedSource, parse_source};
use super::support::{canonical_repository, capabilities};
use super::{Analyzer, mounts};
use crate::language::LanguageRegistry;
use crate::model::{Error, Result, Snapshot};
use std::collections::BTreeSet;
use std::path::Path;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use weavatrix_scan::{
    ContentDiscoveryMode, ContentFileStatus, ContentVisitControl, ContentVisitEvent, ScanOptions,
    ScanReport, Scanner,
};

/// Runs `fetch` across available cores and restores index order before the
/// deterministic integration stage.
pub(super) fn parse_parallel<F>(count: usize, fetch: F) -> Result<Vec<ParsedSource>>
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
                    let mut guard = results
                        .lock()
                        .unwrap_or_else(std::sync::PoisonError::into_inner);
                    guard.push((index, result));
                }
            });
        }
    });
    let mut items = results
        .into_inner()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    items.sort_unstable_by_key(|(index, _)| *index);
    items.into_iter().map(|(_, result)| result).collect()
}

impl Analyzer {
    pub(crate) fn analyze_with_report(
        &self,
        repository: impl AsRef<Path>,
    ) -> Result<(Snapshot, ScanReport)> {
        let repository = canonical_repository(repository.as_ref())?;
        let timing = std::env::var_os("WEAVATRIX_PHASE_TIMING").is_some();
        let started = std::time::Instant::now();
        let parsed = Arc::new(Mutex::new(Vec::<(u64, Result<ParsedSource>)>::new()));
        let sink = Arc::clone(&parsed);
        let visit = Scanner::new(&repository)
            .options(self.scan_options())
            .visit_content_manifest(move |_| {
                let sink = Arc::clone(&sink);
                let registry = LanguageRegistry::default();
                let mut bytes = Vec::new();
                move |event| {
                    match event {
                        ContentVisitEvent::FileStart { file, .. } => {
                            bytes.clear();
                            if let Ok(required) = usize::try_from(file.bytes)
                                && required > bytes.capacity()
                            {
                                bytes.reserve(required - bytes.capacity());
                            }
                        }
                        ContentVisitEvent::Chunk { bytes: chunk, .. } => {
                            bytes.extend_from_slice(chunk);
                        }
                        ContentVisitEvent::FileEnd {
                            file,
                            status: ContentFileStatus::Selected,
                            content_hash,
                            ..
                        } => {
                            let result =
                                parse_source(file.relative, &bytes, content_hash, &registry);
                            sink.lock()
                                .unwrap_or_else(std::sync::PoisonError::into_inner)
                                .push((file.sequence, result));
                        }
                        ContentVisitEvent::FileEnd { .. } => bytes.clear(),
                    }
                    ContentVisitControl::Continue
                }
            })?;
        let scan = visit.into_scan_report();
        let mut parsed = {
            let mut guard = parsed
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            std::mem::take(&mut *guard)
        };
        parsed.sort_unstable_by_key(|(sequence, _)| *sequence);
        let mut parsed = parsed
            .into_iter()
            .map(|(_, result)| result)
            .collect::<Result<Vec<_>>>()?;
        mounts::apply(&mut parsed);
        let parsed_at = started.elapsed();
        let (snapshot, integrated_at, resolved_at) =
            self.integrate_snapshot(&repository, &scan, parsed, &started)?;
        if timing {
            eprintln!(
                "phase-timing one-pass-parse={:.1}ms integrate={:.1}ms resolve={:.1}ms snapshot={:.1}ms",
                parsed_at.as_secs_f64() * 1e3,
                integrated_at.saturating_sub(parsed_at).as_secs_f64() * 1e3,
                resolved_at.saturating_sub(integrated_at).as_secs_f64() * 1e3,
                started.elapsed().saturating_sub(resolved_at).as_secs_f64() * 1e3,
            );
        }
        Ok((snapshot, scan))
    }

    pub(crate) fn scan(
        &self,
        repository: &Path,
        previous: Option<&ScanReport>,
    ) -> Result<ScanReport> {
        let scanner = Scanner::new(repository).options(self.scan_options());
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
        let (snapshot, integrated_at, resolved_at) =
            self.integrate_snapshot(repository, scan, parsed, &started)?;
        if timing {
            eprintln!(
                "phase-timing parse={:.1}ms integrate={:.1}ms resolve={:.1}ms snapshot={:.1}ms",
                parsed_at.as_secs_f64() * 1e3,
                integrated_at.saturating_sub(parsed_at).as_secs_f64() * 1e3,
                resolved_at.saturating_sub(integrated_at).as_secs_f64() * 1e3,
                started.elapsed().saturating_sub(resolved_at).as_secs_f64() * 1e3,
            );
        }
        Ok(snapshot)
    }

    fn integrate_snapshot(
        &self,
        repository: &Path,
        scan: &ScanReport,
        parsed: Vec<ParsedSource>,
        started: &std::time::Instant,
    ) -> Result<(Snapshot, std::time::Duration, std::time::Duration)> {
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
        )?;
        Ok((snapshot, integrated_at, resolved_at))
    }

    fn scan_options(&self) -> ScanOptions {
        let extensions = self
            .languages
            .extensions()
            .map(str::to_owned)
            .collect::<BTreeSet<_>>();
        let mut options = ScanOptions::default().with_extensions(extensions);
        options.max_file_bytes = self.config.max_file_bytes;
        options.content_discovery = ContentDiscoveryMode::BufferedParallel;
        options
    }
}
