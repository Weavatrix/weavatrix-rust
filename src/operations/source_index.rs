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
    if let Some(glob) = glob {
        // An override rule can force-include a file the resident snapshot skipped.
        return filesystem(root, query, options, Some(glob));
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
