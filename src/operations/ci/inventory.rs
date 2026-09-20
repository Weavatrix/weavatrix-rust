use super::ignore::Ignore;
use super::read::{Outcome, open};
use crate::engine::RepositoryState;
use std::collections::BTreeSet;
use std::fs;
use std::path::Path;

const MAX_WALKED: usize = 200;

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
    let ignore = Ignore::load(state.root());
    let mut excluded = Vec::new();
    let mut read = Vec::new();
    for path in candidates(state.root(), &mut excluded) {
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

fn candidates(root: &Path, excluded: &mut Vec<(String, &'static str)>) -> BTreeSet<String> {
    let mut paths = BTreeSet::new();
    for name in ROOT_CONFIGS {
        if root.join(name).is_file() {
            paths.insert((*name).to_owned());
        }
    }
    let mut walked = paths.len();
    walk(
        root,
        &root.join(".github"),
        &mut paths,
        false,
        excluded,
        &mut walked,
    );
    walk(
        root,
        &root.join("scripts"),
        &mut paths,
        true,
        excluded,
        &mut walked,
    );
    paths
}

fn walk(
    root: &Path,
    directory: &Path,
    paths: &mut BTreeSet<String>,
    scripts: bool,
    excluded: &mut Vec<(String, &'static str)>,
    walked: &mut usize,
) {
    if *walked >= MAX_WALKED {
        excluded.push((relative(root, directory), "limit"));
        return;
    }
    if !directory.is_dir() {
        return;
    }
    let Ok(entries) = fs::read_dir(directory) else {
        excluded.push((relative(root, directory), "unreadable_directory"));
        return;
    };
    for entry in entries.flatten() {
        if *walked >= MAX_WALKED {
            excluded.push((relative(root, directory), "limit"));
            return;
        }
        *walked += 1;
        let path = entry.path();
        if fs::symlink_metadata(&path).is_ok_and(|metadata| metadata.file_type().is_symlink()) {
            excluded.push((relative(root, &path), "symlink"));
            continue;
        }
        if path.is_dir() {
            walk(root, &path, paths, scripts, excluded, walked);
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
        let supported = Path::new(&relative)
            .extension()
            .and_then(|ext| ext.to_str())
            .is_some_and(|ext| {
                if scripts {
                    matches!(ext, "sh" | "ps1" | "js" | "mjs" | "cjs" | "ts")
                } else {
                    ext.eq_ignore_ascii_case("yml") || ext.eq_ignore_ascii_case("yaml")
                }
            });
        if supported {
            paths.insert(relative);
        }
    }
}

fn relative(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .to_string_lossy()
        .replace('\\', "/")
}
