use super::support::{normalized_path, parsed_provenance, sanitize_id};
use crate::Result;
use crate::language::{ImportFact, Language};
use std::collections::{BTreeMap, BTreeSet};
use std::ops::Bound;
use std::path::{Component, Path, PathBuf};
use weavatrix_graph::{Edge, EdgeKind, GraphBuilder, Node, NodeId, NodeKind};

pub(super) struct PendingImport {
    pub source: NodeId,
    pub source_path: String,
    pub language: Language,
    pub extractor: &'static str,
    pub import: ImportFact,
}

pub(super) fn resolve(
    graph: &mut GraphBuilder,
    files: &BTreeMap<String, NodeId>,
    repository_label: &str,
    imports: Vec<PendingImport>,
    reexports: Vec<PendingImport>,
) -> Result<()> {
    let context = ResolutionContext::new(files, repository_label, &imports);
    let forwards = resolve_reexports(graph, files, &context, reexports)?;
    resolve_imports(graph, files, &context, imports, &forwards)
}

/// Records re-export evidence and returns the barrel map: a file that
/// forwards another module's surface, so importers of the barrel reach the
/// defining module transitively.
fn resolve_reexports(
    graph: &mut GraphBuilder,
    files: &BTreeMap<String, NodeId>,
    context: &ResolutionContext<'_>,
    reexports: Vec<PendingImport>,
) -> Result<BTreeMap<String, Vec<String>>> {
    let mut forwards = BTreeMap::<String, Vec<String>>::new();
    for item in reexports {
        let Some(target) = context.local_path(&item) else {
            continue;
        };
        if target == item.source_path {
            continue;
        }
        forwards
            .entry(item.source_path.clone())
            .or_default()
            .push(target.clone());
        if let Some(target_id) = files.get(&target) {
            let provenance = parsed_provenance(item.extractor, Some(item.import.span.clone()))?
                .with_detail(format!("re-export; specifier: {}", item.import.target));
            graph.add_edge(Edge::new(
                item.source.clone(),
                target_id.clone(),
                EdgeKind::ReExports,
                provenance,
            ))?;
        }
    }
    Ok(forwards)
}

fn resolve_imports(
    graph: &mut GraphBuilder,
    files: &BTreeMap<String, NodeId>,
    context: &ResolutionContext<'_>,
    imports: Vec<PendingImport>,
    forwards: &BTreeMap<String, Vec<String>>,
) -> Result<()> {
    for item in imports {
        let locals = context.local_targets(&item);
        let is_local = !locals.is_empty();
        let targets = if is_local {
            locals
        } else {
            vec![add_package(graph, &item)?]
        };
        let evidence = if is_local {
            "repository import resolved to a source file"
        } else {
            "external package import"
        };
        for target in targets {
            let provenance = parsed_provenance(item.extractor, Some(item.import.span.clone()))?
                .with_detail(format!("{evidence}; specifier: {}", item.import.target));
            graph.add_edge(Edge::new(
                item.source.clone(),
                target,
                EdgeKind::Imports,
                provenance,
            ))?;
        }
        if is_local && !forwards.is_empty() {
            for defining in context.forwarded(&item, forwards) {
                let Some(target_id) = files.get(&defining) else {
                    continue;
                };
                let provenance = parsed_provenance(item.extractor, Some(item.import.span.clone()))?
                    .with_detail(format!(
                        "import resolved through a re-export chain; specifier: {}",
                        item.import.target
                    ));
                graph.add_edge(Edge::new(
                    item.source.clone(),
                    target_id.clone(),
                    EdgeKind::Imports,
                    provenance,
                ))?;
            }
        }
    }
    Ok(())
}

fn add_package(graph: &mut GraphBuilder, item: &PendingImport) -> Result<NodeId> {
    let name = package_name(&item.language, &item.import.target);
    let package = Node::new(
        format!("package:{}:{}", item.language.as_str(), sanitize_id(&name)),
        name,
        NodeKind::Package,
    )?
    .with_language(item.language.as_str());
    let id = package.id.clone();
    graph.add_node(package)?;
    Ok(id)
}

struct ResolutionContext<'a> {
    files: &'a BTreeMap<String, NodeId>,
    repository_label: String,
    /// Workspace crate roots: normalized crate name -> `src` directory.
    rust_roots: BTreeMap<String, String>,
    /// Java classpath index: `com/x/Y.java` suffix -> repository path.
    java_index: BTreeMap<String, String>,
}

impl<'a> ResolutionContext<'a> {
    fn new(
        files: &'a BTreeMap<String, NodeId>,
        repository_label: &str,
        imports: &[PendingImport],
    ) -> Self {
        let mut rust_roots = BTreeMap::new();
        if imports.iter().any(|item| item.language == Language::Rust) {
            for path in files.keys() {
                let Some(root) = path
                    .strip_suffix("/src/lib.rs")
                    .or_else(|| path.strip_suffix("/src/main.rs"))
                else {
                    continue;
                };
                let Some(name) = Path::new(root).file_name().and_then(|value| value.to_str())
                else {
                    continue;
                };
                rust_roots.insert(normalize_crate(name), format!("{root}/src"));
            }
            if files.contains_key("src/lib.rs") || files.contains_key("src/main.rs") {
                rust_roots.insert(normalize_crate(repository_label), "src".to_owned());
            }
        }
        let mut java_index = BTreeMap::new();
        if imports.iter().any(|item| item.language == Language::Java) {
            for path in files.keys() {
                if !has_extension(path, "java") {
                    continue;
                }
                let key = path
                    .rfind("/java/")
                    .map_or(path.as_str(), |position| &path[position + 6..]);
                java_index.insert(key.to_owned(), path.clone());
            }
        }
        Self {
            files,
            repository_label: repository_label.to_owned(),
            rust_roots,
            java_index,
        }
    }

    fn local_targets(&self, item: &PendingImport) -> Vec<NodeId> {
        match item.language {
            Language::Go => self.go_targets(item),
            Language::Java => self.java_targets(item),
            _ => self
                .first_existing(candidates(item, &self.rust_roots))
                .into_iter()
                .collect(),
        }
    }

    fn first_existing(&self, candidates: Vec<String>) -> Option<NodeId> {
        candidates
            .into_iter()
            .find_map(|candidate| self.files.get(&candidate).cloned())
    }

    /// The repository path a specifier resolves to, if any.
    fn local_path(&self, item: &PendingImport) -> Option<String> {
        candidates(item, &self.rust_roots)
            .into_iter()
            .find(|candidate| self.files.contains_key(candidate))
    }

    /// Files reached by following the barrel chain from this import, bounded
    /// in depth and breadth and safe against cycles.
    fn forwarded(
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
    /// directory when the import path passes through this repository.
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

    /// Resolves a Java import through the classpath-style suffix index,
    /// walking back over trailing type/member segments.
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

/// Ordered resolution candidates, most specific first.
fn candidates(item: &PendingImport, rust_roots: &BTreeMap<String, String>) -> Vec<String> {
    let mut bases = Vec::new();
    let target = clean_specifier(&item.import.target);
    let parent = Path::new(&item.source_path)
        .parent()
        .unwrap_or_else(|| Path::new(""));
    if target.starts_with('.')
        || matches!(item.language, Language::C | Language::Cpp | Language::Bash)
    {
        push_unique(&mut bases, normalize_join(parent, &target));
    }
    match item.language {
        Language::Rust => rust_candidates(&mut bases, item, &target, rust_roots),
        Language::Python => python_candidates(&mut bases, &target, parent),
        Language::JavaScript | Language::TypeScript => {
            // Vite/webpack convention: `@/x` aliases the source root.
            if let Some(rest) = target.strip_prefix("@/") {
                push_unique(&mut bases, format!("src/{rest}"));
            }
        }
        _ => {}
    }
    expand(bases, extensions(&item.language))
}

fn rust_candidates(
    bases: &mut Vec<String>,
    item: &PendingImport,
    target: &str,
    rust_roots: &BTreeMap<String, String>,
) {
    let mut segments = rust_segments(target);
    if segments.is_empty() {
        return;
    }
    let source = Path::new(&item.source_path);
    let mut roots = Vec::new();
    match segments[0] {
        "crate" => {
            segments.remove(0);
            roots.push(rust_src_root(source));
        }
        "self" => {
            segments.remove(0);
            roots.push(rust_module_dir(source));
        }
        "super" => {
            let mut module = rust_module_dir(source);
            while segments.first() == Some(&"super") {
                segments.remove(0);
                module.pop();
            }
            roots.push(module);
        }
        first => {
            roots.push(rust_module_dir(source));
            roots.push(rust_src_root(source));
            if let Some(root) = rust_roots.get(&normalize_crate(first)) {
                let mut member = segments.clone();
                member.remove(0);
                push_prefix_walk(bases, Path::new(root), &member);
            }
        }
    }
    for root in roots {
        push_prefix_walk(bases, &root, &segments);
    }
}

fn push_prefix_walk(bases: &mut Vec<String>, root: &Path, segments: &[&str]) {
    for length in (1..=segments.len()).rev() {
        let mut path = root.to_path_buf();
        for segment in &segments[..length] {
            path.push(segment);
        }
        push_unique(bases, normalized_path(&path));
    }
    if segments.is_empty() {
        push_unique(bases, normalized_path(root));
    }
}

fn rust_segments(target: &str) -> Vec<&str> {
    let module = target.split('{').next().unwrap_or(target);
    module
        .split("::")
        .map(|segment| segment.trim().trim_start_matches("r#"))
        .filter(|segment| {
            !segment.is_empty() && *segment != "*" && !segment.contains(char::is_whitespace)
        })
        .collect()
}

/// The `src` directory of the crate containing `source`, or its parent
/// directory outside a conventional layout.
fn rust_src_root(source: &Path) -> PathBuf {
    let mut current = source.parent().unwrap_or_else(|| Path::new(""));
    loop {
        if current.file_name().is_some_and(|name| name == "src") {
            return current.to_path_buf();
        }
        match current.parent() {
            Some(parent) if parent != current => current = parent,
            _ => {
                return source
                    .parent()
                    .unwrap_or_else(|| Path::new(""))
                    .to_path_buf();
            }
        }
    }
}

/// The directory whose children are this module's submodules.
fn rust_module_dir(source: &Path) -> PathBuf {
    let parent = source
        .parent()
        .unwrap_or_else(|| Path::new(""))
        .to_path_buf();
    let stem = source
        .file_stem()
        .and_then(|value| value.to_str())
        .unwrap_or_default();
    if matches!(stem, "lib" | "main" | "mod") {
        parent
    } else {
        parent.join(stem)
    }
}

fn python_candidates(bases: &mut Vec<String>, target: &str, parent: &Path) {
    if target.starts_with('.') {
        let dots = target
            .chars()
            .take_while(|character| *character == '.')
            .count();
        let mut base = parent.to_path_buf();
        for _ in 1..dots {
            base.pop();
        }
        let segments = target[dots..]
            .split('.')
            .filter(|segment| !segment.is_empty())
            .collect::<Vec<_>>();
        push_prefix_walk(bases, &base, &segments);
        return;
    }
    let segments = target
        .split('.')
        .filter(|segment| !segment.is_empty())
        .collect::<Vec<_>>();
    push_prefix_walk(bases, Path::new(""), &segments);
}

fn push_unique(bases: &mut Vec<String>, value: String) {
    if !value.is_empty() && !bases.contains(&value) {
        bases.push(value);
    }
}

fn expand(bases: Vec<String>, extensions: &[&str]) -> Vec<String> {
    let mut result = Vec::new();
    for base in bases {
        if !result.contains(&base) {
            result.push(base.clone());
        }
        if Path::new(&base).extension().is_none() {
            for extension in extensions {
                for candidate in [
                    format!("{base}.{extension}"),
                    format!("{base}/mod.{extension}"),
                    format!("{base}/index.{extension}"),
                    format!("{base}/__init__.{extension}"),
                ] {
                    if !result.contains(&candidate) {
                        result.push(candidate);
                    }
                }
            }
        }
    }
    result
}

fn extensions(language: &Language) -> &'static [&'static str] {
    match language {
        Language::Rust => &["rs"],
        Language::JavaScript => &["js", "jsx", "mjs", "cjs"],
        Language::TypeScript => &["ts", "tsx", "js", "jsx", "mts", "cts"],
        Language::Python => &["py", "pyi"],
        Language::Go => &["go"],
        Language::C => &["c", "h"],
        Language::Cpp => &["cpp", "cc", "cxx", "h", "hpp", "hh"],
        Language::Bash => &["sh", "bash"],
        _ => &[],
    }
}

fn normalize_crate(name: &str) -> String {
    name.replace('-', "_")
}

fn has_extension(path: &str, extension: &str) -> bool {
    Path::new(path)
        .extension()
        .is_some_and(|value| value.eq_ignore_ascii_case(extension))
}

fn clean_specifier(value: &str) -> String {
    let token = value.split_whitespace().next().unwrap_or(value);
    let token = token.trim_matches(|character| matches!(character, '"' | '\'' | '<' | '>'));
    token.split(['?', '#']).next().unwrap_or(token).to_owned()
}

fn package_name(language: &Language, value: &str) -> String {
    let target = clean_specifier(value);
    match language {
        Language::Rust => target.split("::").next().unwrap_or(&target).to_owned(),
        Language::Python => target.split('.').next().unwrap_or(&target).to_owned(),
        Language::JavaScript | Language::TypeScript if target.starts_with('@') => {
            target.split('/').take(2).collect::<Vec<_>>().join("/")
        }
        Language::JavaScript | Language::TypeScript => {
            target.split('/').next().unwrap_or(&target).to_owned()
        }
        _ => target,
    }
}

fn normalize_join(parent: &Path, value: &str) -> String {
    let joined = parent.join(value.replace('\\', "/"));
    let mut normalized = PathBuf::new();
    for component in joined.components() {
        match component {
            Component::ParentDir => {
                normalized.pop();
            }
            Component::CurDir | Component::Prefix(_) | Component::RootDir => {}
            Component::Normal(value) => normalized.push(value),
        }
    }
    normalized_path(&normalized)
}
