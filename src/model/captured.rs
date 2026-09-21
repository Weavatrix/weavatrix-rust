//! Immutable, bounded bytes used by configuration-driven operations.

use super::digest::{sha3_256, sha256};
use serde::Serialize;
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;
use weavatrix_scan::ScanReport;

const MAX_FILE_BYTES: u64 = 2_000_000;
const MAX_TOTAL_BYTES: usize = 64 * 1024 * 1024;
const MAX_WALKED_CONFIGS: usize = 500;

const PROBED_NAMES: &[&str] = &[
    "Cargo.toml",
    "Cargo.lock",
    "package.json",
    "pnpm-workspace.yaml",
    "lerna.json",
    "go.mod",
    "go.sum",
    "go.work",
    "pyproject.toml",
    "turbo.json",
    "nx.json",
];

#[derive(Clone, Debug)]
pub(crate) struct CapturedFile {
    pub path: String,
    pub content_digest: String,
    pub bytes: Vec<u8>,
}

#[derive(Clone, Debug, Serialize)]
pub(crate) struct CaptureExclusion {
    pub path: String,
    pub reason: &'static str,
}

#[derive(Clone, Debug)]
pub(crate) struct EvidenceSnapshot {
    source_revision: String,
    generation: String,
    files: BTreeMap<String, CapturedFile>,
    excluded: Vec<CaptureExclusion>,
    complete: bool,
    bytes: usize,
}

#[derive(Debug, Serialize)]
pub(crate) struct EvidenceReceipt<'a> {
    pub source_revision: &'a str,
    pub generation: &'a str,
    pub files_captured: usize,
    pub bytes_captured: usize,
    pub complete: bool,
    pub excluded: &'a [CaptureExclusion],
}

impl EvidenceSnapshot {
    #[must_use]
    pub fn capture(root: &Path, scan: &ScanReport) -> Self {
        let candidates = candidates(root, scan);
        let expected = scan
            .files
            .iter()
            .filter_map(|file| Some((file.relative.as_str(), file.content_hash.as_deref()?)))
            .collect::<BTreeMap<_, _>>();
        let mut files = BTreeMap::new();
        let mut excluded = Vec::new();
        let mut bytes = 0usize;
        for relative in candidates {
            match capture_file(root, &relative, expected.get(relative.as_str()).copied()) {
                Capture::Missing => {}
                Capture::Excluded(reason) => excluded.push(CaptureExclusion {
                    path: relative,
                    reason,
                }),
                Capture::File(file) => {
                    if bytes.saturating_add(file.bytes.len()) > MAX_TOTAL_BYTES {
                        excluded.push(CaptureExclusion {
                            path: relative,
                            reason: "total_bytes_limit",
                        });
                    } else {
                        bytes += file.bytes.len();
                        files.insert(relative, file);
                    }
                }
            }
        }
        excluded.sort_by(|left, right| (&left.path, left.reason).cmp(&(&right.path, right.reason)));
        let generation = generation(&scan.revision, &files, &excluded);
        Self {
            source_revision: scan.revision.clone(),
            generation,
            complete: scan.complete && excluded.is_empty(),
            files,
            excluded,
            bytes,
        }
    }

    pub fn file(&self, path: &str) -> Option<&CapturedFile> {
        self.files.get(path)
    }

    pub fn text(&self, path: &str) -> Option<&str> {
        std::str::from_utf8(&self.files.get(path)?.bytes).ok()
    }

    pub fn paths(&self) -> impl Iterator<Item = &str> {
        self.files.keys().map(String::as_str)
    }

    pub fn exclusion(&self, path: &str) -> Option<&CaptureExclusion> {
        self.excluded.iter().find(|item| item.path == path)
    }

    pub fn generation(&self) -> &str {
        &self.generation
    }

    pub fn receipt(&self) -> EvidenceReceipt<'_> {
        EvidenceReceipt {
            source_revision: &self.source_revision,
            generation: &self.generation,
            files_captured: self.files.len(),
            bytes_captured: self.bytes,
            complete: self.complete,
            excluded: &self.excluded,
        }
    }
}

enum Capture {
    Missing,
    Excluded(&'static str),
    File(CapturedFile),
}

fn capture_file(root: &Path, relative: &str, expected_hash: Option<&str>) -> Capture {
    let joined = root.join(relative);
    let Ok(link_metadata) = std::fs::symlink_metadata(&joined) else {
        return Capture::Missing;
    };
    if link_metadata.file_type().is_symlink() {
        return Capture::Excluded("symlink");
    }
    let Ok(canonical) = joined.canonicalize() else {
        return Capture::Excluded("unreadable");
    };
    if !canonical.starts_with(root) {
        return Capture::Excluded("path_escape");
    }
    if !link_metadata.is_file() {
        return Capture::Missing;
    }
    if link_metadata.len() > MAX_FILE_BYTES {
        return Capture::Excluded("too_large");
    }
    let (Ok(first), Ok(second)) = (std::fs::read(&canonical), std::fs::read(&canonical)) else {
        return Capture::Excluded("unreadable");
    };
    if first != second {
        return Capture::Excluded("concurrent_modification");
    }
    if expected_hash.is_some_and(|expected| sha256(&first) != expected) {
        return Capture::Excluded("scan_content_mismatch");
    }
    Capture::File(CapturedFile {
        path: relative.to_owned(),
        content_digest: sha3_256(&first),
        bytes: first,
    })
}

fn candidates(root: &Path, scan: &ScanReport) -> BTreeSet<String> {
    let mut candidates = BTreeSet::from([
        ".gitignore".to_owned(),
        ".weavatrix/architecture.json".to_owned(),
    ]);
    let mut directories = BTreeSet::from([String::new()]);
    for file in &scan.files {
        let relative = file.relative.replace('\\', "/");
        if should_capture_scanned(&relative) {
            candidates.insert(relative.clone());
        }
        let mut parent = Path::new(&relative).parent();
        while let Some(path) = parent {
            directories.insert(path.to_string_lossy().replace('\\', "/"));
            parent = path.parent();
        }
    }
    for directory in directories {
        for name in PROBED_NAMES {
            candidates.insert(join(&directory, name));
        }
    }
    let mut walked = 0;
    collect_tree(root, &root.join(".github"), &mut candidates, &mut walked);
    collect_tree(root, &root.join("scripts"), &mut candidates, &mut walked);
    candidates
}

fn collect_tree(
    root: &Path,
    directory: &Path,
    candidates: &mut BTreeSet<String>,
    walked: &mut usize,
) {
    if *walked >= MAX_WALKED_CONFIGS || !directory.is_dir() {
        return;
    }
    let Ok(entries) = std::fs::read_dir(directory) else {
        return;
    };
    for entry in entries.flatten() {
        if *walked >= MAX_WALKED_CONFIGS {
            return;
        }
        *walked += 1;
        let path = entry.path();
        if path.is_dir() {
            collect_tree(root, &path, candidates, walked);
        } else if let Some(relative) = relative(root, &path)
            && should_capture_scanned(&relative)
        {
            candidates.insert(relative);
        }
    }
}

fn should_capture_scanned(path: &str) -> bool {
    let name = path.rsplit('/').next().unwrap_or(path);
    let source_extension = Path::new(name)
        .extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| matches!(extension, "go" | "py"));
    PROBED_NAMES.contains(&name)
        || source_extension
        || name.starts_with("tsconfig")
            && Path::new(name)
                .extension()
                .is_some_and(|extension| extension.eq_ignore_ascii_case("json"))
        || path.starts_with(".github/")
        || path.starts_with("scripts/")
        || path.starts_with("tests/architecture/")
        || matches!(
            name,
            "rustfmt.toml" | ".rustfmt.toml" | "clippy.toml" | ".clippy.toml"
        )
}

fn generation(
    revision: &str,
    files: &BTreeMap<String, CapturedFile>,
    excluded: &[CaptureExclusion],
) -> String {
    let mut identity = revision.as_bytes().to_vec();
    for (path, file) in files {
        identity.extend_from_slice(path.as_bytes());
        identity.push(0);
        identity.extend_from_slice(file.content_digest.as_bytes());
        identity.push(0);
    }
    for item in excluded {
        identity.extend_from_slice(item.path.as_bytes());
        identity.push(0);
        identity.extend_from_slice(item.reason.as_bytes());
        identity.push(0);
    }
    sha3_256(&identity)
}

fn relative(root: &Path, path: &Path) -> Option<String> {
    Some(path.strip_prefix(root).ok()?.to_str()?.replace('\\', "/"))
}

fn join(directory: &str, name: &str) -> String {
    if directory.is_empty() {
        name.to_owned()
    } else {
        format!("{directory}/{name}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scanned_capture_policy_includes_p2_inputs_but_not_arbitrary_sources() {
        assert!(should_capture_scanned("packages/a/Cargo.toml"));
        assert!(should_capture_scanned("packages/a/tsconfig.build.json"));
        assert!(should_capture_scanned(".github/workflows/ci.yml"));
        assert!(should_capture_scanned("tests/architecture/self.rs"));
        assert!(!should_capture_scanned("src/lib.rs"));
    }
}
