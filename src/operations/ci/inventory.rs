use super::ignore::Ignore;
use super::read::captured;
use crate::engine::RepositoryState;
use std::path::Path;

const MAX_CONFIGS: usize = 200;

const ROOT_CONFIGS: &[&str] = &[
    "Cargo.toml",
    "package.json",
    "rustfmt.toml",
    ".rustfmt.toml",
    "clippy.toml",
    ".clippy.toml",
    ".weavatrix/architecture.json",
];

pub(super) struct Inventory {
    pub read: Vec<super::read::Loaded>,
    pub excluded: Vec<(String, &'static str)>,
}

pub(super) fn collect(state: &RepositoryState) -> Inventory {
    let ignore = Ignore::load(state.evidence().text(".gitignore").unwrap_or_default());
    let candidates = state
        .evidence()
        .paths()
        .filter(|path| is_candidate(path))
        .collect::<Vec<_>>();
    let mut excluded = state
        .evidence()
        .receipt()
        .excluded
        .iter()
        .filter(|item| is_candidate(&item.path))
        .map(|item| (item.path.clone(), item.reason))
        .collect::<Vec<_>>();
    let mut read = Vec::new();
    for path in candidates.iter().take(MAX_CONFIGS) {
        if ignore.excludes(path) {
            excluded.push(((*path).to_owned(), "gitignore"));
            continue;
        }
        if let Some(file) = state.evidence().file(path) {
            read.push(captured(file));
        }
    }
    if candidates.len() > MAX_CONFIGS {
        excluded.extend(
            candidates
                .iter()
                .skip(MAX_CONFIGS)
                .map(|path| ((*path).to_owned(), "limit")),
        );
    }
    Inventory { read, excluded }
}

fn is_candidate(path: &str) -> bool {
    if ROOT_CONFIGS.contains(&path) {
        return true;
    }
    let extension = Path::new(path)
        .extension()
        .and_then(|ext| ext.to_str())
        .unwrap_or_default();
    path.starts_with(".github/") && matches!(extension, "yml" | "yaml")
        || path.starts_with("scripts/")
            && matches!(extension, "sh" | "ps1" | "js" | "mjs" | "cjs" | "ts")
}
