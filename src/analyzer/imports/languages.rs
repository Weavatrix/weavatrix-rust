use super::super::support::{normalize_join, normalized_path};
use super::PendingImport;
use super::candidates::{expand, push_unique};
use super::paths::clean_specifier;
use crate::language::Language;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

pub(super) fn cargo_package_name(manifest: &str) -> Option<String> {
    let mut package = false;
    for line in manifest.lines() {
        let line = line.split('#').next().unwrap_or_default().trim();
        if line.starts_with('[') {
            package = line == "[package]";
            continue;
        }
        if package
            && let Some((key, value)) = line.split_once('=')
            && key.trim() == "name"
        {
            let name = value.trim().trim_matches(['"', '\'']);
            if !name.is_empty() {
                return Some(name.to_owned());
            }
        }
    }
    None
}

/// Ordered language-native resolution candidates, most specific first.
pub(super) fn candidate_paths(
    item: &PendingImport,
    rust_roots: &BTreeMap<String, String>,
) -> Vec<String> {
    let mut bases = Vec::new();
    let target = clean_specifier(&item.import.target);
    let parent = Path::new(&item.source_path)
        .parent()
        .unwrap_or_else(|| Path::new(""));
    if target.starts_with('.')
        || matches!(
            item.language,
            Language::C | Language::Cpp | Language::Bash | Language::Protobuf
        )
        || item.language.as_str() == "markdown"
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
    let mut crate_roots = Vec::new();
    match segments[0] {
        "crate" => {
            segments.remove(0);
            let root = rust_src_root(source);
            roots.push(root.clone());
            crate_roots.push(root);
        }
        "self" => {
            segments.remove(0);
            let module = rust_module_dir(source);
            roots.push(module.clone());
            crate_roots.push(module);
        }
        "super" => {
            let mut module = rust_module_dir(source);
            while segments.first() == Some(&"super") {
                segments.remove(0);
                module.pop();
            }
            roots.push(module.clone());
            crate_roots.push(module);
        }
        first => {
            roots.push(rust_module_dir(source));
            roots.push(rust_src_root(source));
            if let Some(root) = rust_roots.get(&normalize_crate(first)) {
                let mut member = segments.clone();
                member.remove(0);
                push_prefix_walk(bases, Path::new(root), &member);
                push_rust_crate_root(bases, Path::new(root));
            }
        }
    }
    for root in roots {
        push_prefix_walk(bases, &root, &segments);
    }
    for root in crate_roots {
        push_rust_crate_root(bases, &root);
    }
}

fn push_rust_crate_root(bases: &mut Vec<String>, root: &Path) {
    push_unique(bases, normalized_path(&root.with_extension("rs")));
    push_unique(bases, normalized_path(&root.join("mod.rs")));
    push_unique(bases, normalized_path(&root.join("lib.rs")));
    push_unique(bases, normalized_path(&root.join("main.rs")));
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
    // Independently executable Python applications put their script directory
    // on `sys.path`, before the repository-root fallback.
    push_prefix_walk(bases, parent, &segments);
    push_prefix_walk(bases, Path::new(""), &segments);
}

pub(super) fn extensions(language: &Language) -> &'static [&'static str] {
    match language {
        Language::Rust => &["rs"],
        Language::JavaScript => &["js", "jsx", "mjs", "cjs"],
        Language::TypeScript => &["ts", "tsx", "js", "jsx", "mts", "cts"],
        Language::Python => &["py", "pyi"],
        Language::Go => &["go"],
        Language::C => &["c", "h"],
        Language::Cpp => &["cpp", "cc", "cxx", "h", "hpp", "hh"],
        Language::Bash => &["sh", "bash"],
        Language::Protobuf => &["proto"],
        Language::Swift => &["swift"],
        _ => &[],
    }
}

pub(super) fn normalize_crate(name: &str) -> String {
    name.replace('-', "_")
}
