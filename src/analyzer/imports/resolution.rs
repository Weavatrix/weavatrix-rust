use super::PendingImport;
use super::candidates::{expand, typescript_runtime_fallbacks};
use super::configuration::ScriptResolver;
use super::languages::{candidate_paths, cargo_package_name, extensions, normalize_crate};
use super::paths::{clean_specifier, has_extension};
use crate::language::Language;
use std::collections::{BTreeMap, BTreeSet};
use std::ops::Bound;
use std::path::Path;
use weavatrix_graph::NodeId;

/// Languages whose runtimes load modules from case-insensitive filesystems,
/// so a unique case-insensitive match is how the program actually runs.
fn folding_resolver(language: &Language) -> bool {
    matches!(language, Language::JavaScript | Language::TypeScript)
        || matches!(language.as_str(), "html" | "css")
}

pub(super) struct ResolutionContext<'a> {
    files: &'a BTreeMap<String, NodeId>,
    root: std::path::PathBuf,
    repository_label: String,
    /// Workspace crate roots: normalized crate name -> `src` directory.
    rust_roots: BTreeMap<String, String>,
    /// Java classpath index: `com/x/Y.java` suffix -> repository path.
    java_index: BTreeMap<String, String>,
    /// Case-folded path -> unique indexed path, for resolvers that load from
    /// case-insensitive filesystems. `None` marks a case collision.
    folded_files: BTreeMap<String, Option<String>>,
    script: ScriptResolver,
}

impl<'a> ResolutionContext<'a> {
    pub(super) fn new(
        files: &'a BTreeMap<String, NodeId>,
        repository_label: &str,
        root: &Path,
        imports: &[PendingImport],
    ) -> Self {
        let script = if imports
            .iter()
            .any(|item| matches!(item.language, Language::JavaScript | Language::TypeScript))
        {
            ScriptResolver::load(root, files)
        } else {
            ScriptResolver::default()
        };
        let mut rust_roots = BTreeMap::new();
        if imports.iter().any(|item| item.language == Language::Rust) {
            for path in files.keys() {
                let Some(crate_root) = path
                    .strip_suffix("/src/lib.rs")
                    .or_else(|| path.strip_suffix("/src/main.rs"))
                else {
                    continue;
                };
                let Some(name) = Path::new(crate_root)
                    .file_name()
                    .and_then(|value| value.to_str())
                else {
                    continue;
                };
                rust_roots.insert(normalize_crate(name), format!("{crate_root}/src"));
                if let Ok(manifest) =
                    std::fs::read_to_string(root.join(crate_root).join("Cargo.toml"))
                    && let Some(package) = cargo_package_name(&manifest)
                {
                    rust_roots.insert(normalize_crate(&package), format!("{crate_root}/src"));
                }
            }
            if files.contains_key("src/lib.rs") || files.contains_key("src/main.rs") {
                rust_roots.insert(normalize_crate(repository_label), "src".to_owned());
                if let Ok(manifest) = std::fs::read_to_string(root.join("Cargo.toml"))
                    && let Some(package) = cargo_package_name(&manifest)
                {
                    rust_roots.insert(normalize_crate(&package), "src".to_owned());
                }
            }
        }
        let mut java_index = BTreeMap::new();
        if imports.iter().any(|item| item.language == Language::Java) {
            for path in files.keys() {
                if !has_extension(path, "java") {
                    continue;
                }
                let key = ["/src/main/java/", "/src/test/java/", "/src/", "/java/"]
                    .into_iter()
                    .find_map(|marker| {
                        path.rfind(marker)
                            .map(|position| &path[position + marker.len()..])
                    })
                    .unwrap_or(path.as_str());
                java_index.insert(key.to_owned(), path.clone());
            }
        }
        let mut folded_files = BTreeMap::new();
        if imports.iter().any(|item| folding_resolver(&item.language)) {
            for path in files.keys() {
                folded_files
                    .entry(path.to_ascii_lowercase())
                    .and_modify(|unique| *unique = None)
                    .or_insert_with(|| Some(path.clone()));
            }
        }
        Self {
            files,
            root: root.to_path_buf(),
            repository_label: repository_label.to_owned(),
            rust_roots,
            java_index,
            folded_files,
            script,
        }
    }

    /// The first candidate that exists on disk even though the scan did not
    /// index it - a real repository file under an excluded directory such as
    /// a source folder named `coverage`.
    pub(super) fn on_disk(&self, item: &PendingImport) -> Option<String> {
        self.candidate_paths(item)
            .into_iter()
            .take(8)
            .find(|candidate| self.root.join(candidate).is_file())
    }

    /// The indexed path a candidate names, including the unique
    /// case-insensitive match Node and browsers accept on the filesystems
    /// these files are loaded from.
    fn existing(&self, candidate: &str, fold: bool) -> Option<String> {
        if self.files.contains_key(candidate) {
            return Some(candidate.to_owned());
        }
        if !fold {
            return None;
        }
        self.folded_files
            .get(&candidate.to_ascii_lowercase())?
            .clone()
    }

    pub(super) fn local_targets(&self, item: &PendingImport) -> Vec<NodeId> {
        match item.language {
            Language::Go => self.go_targets(item),
            Language::Java => self.java_targets(item),
            _ => self
                .first_existing(self.candidate_paths(item), folding_resolver(&item.language))
                .into_iter()
                .collect(),
        }
    }

    /// Resolution candidates, most specific first: language-native forms,
    /// then whatever the project's own resolver configuration allows.
    fn candidate_paths(&self, item: &PendingImport) -> Vec<String> {
        let mut candidates = candidate_paths(item, &self.rust_roots);
        if matches!(item.language, Language::JavaScript | Language::TypeScript) {
            let specifier = clean_specifier(&item.import.target);
            let extensions = extensions(&item.language);
            for base in self.script.bases(&specifier) {
                for candidate in expand(vec![base], extensions) {
                    if !candidates.contains(&candidate) {
                        candidates.push(candidate);
                    }
                }
            }
            if item.language == Language::TypeScript {
                candidates = typescript_runtime_fallbacks(candidates, &specifier);
            }
        }
        candidates
    }

    fn first_existing(&self, candidates: Vec<String>, fold: bool) -> Option<NodeId> {
        candidates
            .into_iter()
            .find_map(|candidate| self.existing(&candidate, fold))
            .and_then(|path| self.files.get(&path).cloned())
    }

    pub(super) fn local_path(&self, item: &PendingImport) -> Option<String> {
        let fold = folding_resolver(&item.language);
        self.candidate_paths(item)
            .into_iter()
            .find_map(|candidate| self.existing(&candidate, fold))
    }

    /// Files reached by following the barrel chain from this import, bounded
    /// in depth and breadth and safe against cycles.
    pub(super) fn forwarded(
        &self,
        item: &PendingImport,
        forwards: &BTreeMap<String, Vec<String>>,
    ) -> Vec<String> {
        const MAX_DEPTH: usize = 4;
        const MAX_TARGETS: usize = 16;
        let Some(entry) = self.local_path(item) else {
            return Vec::new();
        };
        let mut seen = BTreeSet::from([entry.clone()]);
        let mut frontier = vec![entry];
        let mut result = Vec::new();
        for _ in 0..MAX_DEPTH {
            let mut next = Vec::new();
            for current in frontier.drain(..) {
                for target in forwards.get(&current).into_iter().flatten() {
                    if seen.insert(target.clone()) {
                        result.push(target.clone());
                        next.push(target.clone());
                    }
                }
            }
            if next.is_empty() || result.len() >= MAX_TARGETS {
                break;
            }
            frontier = next;
        }
        result.truncate(MAX_TARGETS);
        result
    }

    /// Resolves a Go import to every direct source file of the named package
    /// directory when the path passes through this repository.
    fn go_targets(&self, item: &PendingImport) -> Vec<NodeId> {
        let target = clean_specifier(&item.import.target);
        let segments = target.split('/').collect::<Vec<_>>();
        let Some(position) = segments
            .iter()
            .position(|segment| *segment == self.repository_label)
        else {
            return Vec::new();
        };
        let directory = segments[position + 1..].join("/");
        if directory.is_empty() {
            return Vec::new();
        }
        let prefix = format!("{directory}/");
        self.files
            .range::<String, _>((Bound::Included(&prefix), Bound::Unbounded))
            .take_while(|(path, _)| path.starts_with(&prefix))
            .filter(|(path, _)| has_extension(path, "go") && !path[prefix.len()..].contains('/'))
            .map(|(_, id)| id.clone())
            .collect()
    }

    /// Resolves a Java import through the classpath-style suffix index.
    fn java_targets(&self, item: &PendingImport) -> Vec<NodeId> {
        let target = clean_specifier(&item.import.target);
        let segments = target
            .split('.')
            .filter(|segment| !segment.is_empty() && *segment != "*")
            .collect::<Vec<_>>();
        for length in (1..=segments.len()).rev() {
            let key = format!("{}.java", segments[..length].join("/"));
            if let Some(path) = self.java_index.get(&key)
                && let Some(id) = self.files.get(path)
            {
                return vec![id.clone()];
            }
        }
        Vec::new()
    }
}
