use super::ignore::Ignore;
use super::read::{Outcome, open};
use crate::engine::RepositoryState;
use std::collections::BTreeSet;
use std::fs;
use std::path::Path;

const MAX_WALKED: usize = 200;

const ROOT_CONFIGS: &[&str] = &[
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
    let ignore = Ignore::load(state.root());
    let mut excluded = Vec::new();
    let mut read = Vec::new();
    for path in candidates(state.root()) {
        if ignore.excludes(&path) {
            excluded.push((path, "gitignore"));
            continue;
        }
        match open(state.root(), &path) {
            Outcome::Read(loaded) => read.push(loaded),
            Outcome::Excluded { path, reason } if reason != "missing" => {
                excluded.push((path, reason));
            }
            Outcome::Excluded { .. } => {}
        }
    }
    Inventory { read, excluded }
}

fn candidates(root: &Path) -> BTreeSet<String> {
    let mut paths = BTreeSet::new();
    for name in ROOT_CONFIGS {
        if root.join(name).is_file() {
            paths.insert((*name).to_owned());
        }
    }
    walk(root, &root.join(".github"), &mut paths);
    paths
}

fn walk(root: &Path, directory: &Path, paths: &mut BTreeSet<String>) {
    if paths.len() >= MAX_WALKED || !directory.is_dir() {
        return;
    }
    let Ok(entries) = fs::read_dir(directory) else {
        return;
    };
    for entry in entries.flatten() {
        if paths.len() >= MAX_WALKED {
            return;
        }
        let path = entry.path();
        if path.is_dir() {
            walk(root, &path, paths);
            continue;
        }
        let Some(relative) = path
            .strip_prefix(root)
            .ok()
            .and_then(|value| value.to_str())
        else {
            continue;
        };
        let relative = relative.replace('\\', "/");
        let yaml = Path::new(&relative)
            .extension()
            .and_then(|ext| ext.to_str())
            .is_some_and(|ext| ext.eq_ignore_ascii_case("yml") || ext.eq_ignore_ascii_case("yaml"));
        if yaml {
            paths.insert(relative);
        }
    }
}
