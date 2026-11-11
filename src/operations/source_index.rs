use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};
use weavatrix_search::{
    IndexOptions, LiveIndex, LiveIndexOptions, SearchOptions, SearchQuery, SearchReport,
    SearchWarning, SearchWarningKind, Searcher, recommended_scan_options,
};

/// One resident index per repository for the life of this process.
///
/// The first `search_code` builds it. Later calls read that snapshot.
/// A graph refresh must not drop it, so it stays here rather than on
/// `RepositoryState`.
static RESIDENT: OnceLock<Mutex<BTreeMap<PathBuf, LiveIndex>>> = OnceLock::new();

pub(super) fn execute(
    root: &Path,
    query: SearchQuery,
    options: &SearchOptions,
    glob: Option<&str>,
) -> Result<SearchReport, String> {
    let glob = glob.map(str::trim).filter(|pattern| !pattern.is_empty());
    if let Some(glob) = glob {
        return execute_glob(root, query, options, glob);
    }
    match resident(root, query.clone(), options.clone()) {
        Ok(report) => Ok(report),
        Err(error) => {
            let mut report = filesystem(root, query, options, None)?;
            report.warnings.push(SearchWarning {
                path: String::new(),
                kind: SearchWarningKind::Limit,
                message: format!("resident index unavailable, scanned the filesystem: {error}"),
            });
            Ok(report)
        }
    }
}

fn execute_glob(
    root: &Path,
    query: SearchQuery,
    options: &SearchOptions,
    glob: &str,
) -> Result<SearchReport, String> {
    let asked = options.max_results.max(1);
    let widened = options
        .clone()
        .with_max_results(asked.saturating_mul(32).clamp(asked, 10_000));
    let Ok(mut report) = resident(root, query.clone(), widened) else {
        return filesystem(root, query, options, Some(glob));
    };
    report.matches.retain(|item| path_matches_glob(&item.path, glob));
    if report.truncated && report.matches.len() < asked {
        return filesystem(root, query, options, Some(glob));
    }
    if report.matches.len() > asked {
        report.matches.truncate(asked);
        report.truncated = true;
    }
    recount(&mut report, glob);
    Ok(report)
}

fn recount(report: &mut SearchReport, glob: &str) {
    let mut lines = std::collections::BTreeSet::new();
    let mut files = std::collections::BTreeSet::new();
    let mut occurrences = 0u64;
    for item in &report.matches {
        lines.insert((item.path.clone(), item.line_number));
        files.insert(item.path.clone());
        occurrences =
            occurrences.saturating_add(u64::try_from(item.spans.len().max(1)).unwrap_or(1));
    }
    report.matching_lines = u64::try_from(lines.len()).unwrap_or(u64::MAX);
    report.files_with_matches = u64::try_from(files.len()).unwrap_or(u64::MAX);
    report.occurrences = occurrences;
    report
        .matched_files
        .retain(|item| path_matches_glob(&item.path, glob));
}

fn path_matches_glob(path: &str, pattern: &str) -> bool {
    let path = path.replace('\\', "/");
    let pattern = pattern.trim().trim_start_matches("./").replace('\\', "/");
    if pattern.is_empty() {
        return true;
    }
    if !pattern.contains('/') {
        let name = path.rsplit('/').next().unwrap_or(path.as_str());
        return glob_match(name.as_bytes(), pattern.as_bytes());
    }
    glob_match(path.as_bytes(), pattern.as_bytes())
}

fn glob_match(text: &[u8], pattern: &[u8]) -> bool {
    fn rec(text: &[u8], pattern: &[u8]) -> bool {
        if pattern.is_empty() {
            return text.is_empty();
        }
        if pattern.starts_with(b"**") {
            let mut rest = &pattern[2..];
            if rest.starts_with(b"/") {
                rest = &rest[1..];
            }
            if rec(text, rest) {
                return true;
            }
            for (index, _) in text.iter().enumerate() {
                if rec(&text[index + 1..], rest) {
                    return true;
                }
            }
            return false;
        }
        if text.is_empty() {
            return pattern.iter().all(|byte| *byte == b'*');
        }
        if pattern[0] == b'*' {
            if rec(text, &pattern[1..]) {
                return true;
            }
            return text[0] != b'/' && rec(&text[1..], pattern);
        }
        if pattern[0] == b'?' {
            return text[0] != b'/' && rec(&text[1..], &pattern[1..]);
        }
        text[0].eq_ignore_ascii_case(&pattern[0]) && rec(&text[1..], &pattern[1..])
    }
    rec(text, pattern)
}

fn resident(
    root: &Path,
    query: SearchQuery,
    options: SearchOptions,
) -> Result<SearchReport, String> {
    let key = canonical_root(root);
    let indexes = RESIDENT.get_or_init(|| Mutex::new(BTreeMap::new()));
    let mut guard = indexes
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    if !guard.contains_key(&key) {
        guard.insert(key.clone(), open_live(&key)?);
    }
    let Some(index) = guard.get(&key) else {
        return Err("resident search index disappeared before the query".to_owned());
    };
    index
        .search(query, options)
        .map_err(|error| error.to_string())
}

fn open_live(root: &Path) -> Result<LiveIndex, String> {
    let directory = index_directory(root);
    std::fs::create_dir_all(&directory).map_err(|error| {
        format!(
            "cannot create search index directory {}: {error}",
            directory.display()
        )
    })?;
    let index_path = directory.join("index");
    let roots = [root.to_path_buf()];
    let scan = recommended_scan_options(&roots, &SearchOptions::default());
    // Rebuild once per process so edits made while this process was down are
    // not served from the previous snapshot. The watcher covers edits after that.
    let live = LiveIndexOptions::default();
    LiveIndex::start(index_path, roots, scan, IndexOptions::default(), live)
        .map_err(|error| error.to_string())
}

fn filesystem(
    root: &Path,
    query: SearchQuery,
    options: &SearchOptions,
    glob: Option<&str>,
) -> Result<SearchReport, String> {
    let mut searcher = Searcher::new(root, query).options(options.clone());
    if let Some(glob) = glob {
        let roots = [root.to_path_buf()];
        let mut scan = recommended_scan_options(&roots, options);
        scan.override_rules.push(glob.to_owned());
        searcher = searcher.scan_options(scan);
    }
    searcher.search().map_err(|error| error.to_string())
}

fn canonical_root(root: &Path) -> PathBuf {
    root.canonicalize().unwrap_or_else(|_| root.to_path_buf())
}

fn index_directory(root: &Path) -> PathBuf {
    let base = if let Some(home) = std::env::var_os("WEAVATRIX_SEARCH_HOME") {
        PathBuf::from(home)
    } else if under_temp(root) {
        std::env::temp_dir().join("weavatrix-search-indexes")
    } else {
        user_search_home()
    };
    base.join(fingerprint(root))
}

fn user_search_home() -> PathBuf {
    let home = std::env::var_os("USERPROFILE")
        .or_else(|| std::env::var_os("HOME"))
        .map_or_else(std::env::temp_dir, PathBuf::from);
    home.join(".weavatrix").join("search")
}

fn under_temp(root: &Path) -> bool {
    let temp = canonical_root(&std::env::temp_dir());
    let root = canonical_root(root);
    root.starts_with(temp.as_path()) || strip_verbatim(&root).starts_with(strip_verbatim(&temp))
}

fn strip_verbatim(path: &Path) -> PathBuf {
    let text = path.to_string_lossy();
    let stripped = text.strip_prefix(r"\\?\").unwrap_or(&text);
    PathBuf::from(stripped)
}

fn fingerprint(root: &Path) -> String {
    let mut hasher = Sha256::new();
    hasher.update(b"weavatrix-search-v1\0");
    hasher.update(root.as_os_str().as_encoded_bytes());
    let mut encoded = String::with_capacity(64);
    for byte in hasher.finalize() {
        let _ = write!(encoded, "{byte:02x}");
    }
    encoded
}

#[cfg(test)]
mod tests {
    use super::path_matches_glob;

    #[test]
    fn a_basename_glob_matches_the_file_name() {
        assert!(path_matches_glob("src/server.go", "*.go"));
        assert!(!path_matches_glob("src/server.go", "*.rs"));
        assert!(path_matches_glob("src/nested/app.rs", "**/*.rs"));
        assert!(!path_matches_glob("src/nested/app.rs", "src/*.rs"));
        assert!(path_matches_glob("src/app.rs", "src/*.rs"));
        assert!(path_matches_glob("src/app.rs", "*.rs"));
    }
}
