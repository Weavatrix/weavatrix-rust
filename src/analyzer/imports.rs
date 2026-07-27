use super::support::{normalized_path, parsed_provenance, sanitize_id};
use crate::Result;
use crate::language::{ImportFact, Language};
use std::collections::{BTreeMap, BTreeSet};
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
    imports: Vec<PendingImport>,
) -> Result<()> {
    for item in imports {
        let local = local_target(files, &item);
        let is_local = local.is_some();
        let target = local.map_or_else(|| add_package(graph, &item), Ok)?;
        let evidence = if is_local {
            "repository import resolved to a source file"
        } else {
            "external package import"
        };
        let provenance = parsed_provenance(item.extractor, Some(item.import.span))?
            .with_detail(format!("{evidence}; specifier: {}", item.import.target));
        graph.add_edge(Edge::new(
            item.source,
            target,
            EdgeKind::Imports,
            provenance,
        ))?;
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

fn local_target(files: &BTreeMap<String, NodeId>, item: &PendingImport) -> Option<NodeId> {
    candidates(item)
        .into_iter()
        .find_map(|candidate| files.get(&candidate).cloned())
}

fn candidates(item: &PendingImport) -> BTreeSet<String> {
    let mut bases = BTreeSet::new();
    let target = clean_specifier(&item.import.target);
    let parent = Path::new(&item.source_path)
        .parent()
        .unwrap_or_else(|| Path::new(""));
    if target.starts_with('.')
        || matches!(item.language, Language::C | Language::Cpp | Language::Bash)
    {
        bases.insert(normalize_join(parent, &target));
    }
    if item.language == Language::Rust {
        if let Some(value) = target.strip_prefix("crate::") {
            bases.insert(format!("src/{}", value.replace("::", "/")));
        } else if let Some(value) = target.strip_prefix("self::") {
            bases.insert(normalize_join(parent, &value.replace("::", "/")));
        }
    }
    if item.language == Language::Python && target.starts_with('.') {
        let dots = target
            .chars()
            .take_while(|character| *character == '.')
            .count();
        let mut base = parent.to_path_buf();
        for _ in 1..dots {
            base.pop();
        }
        let module = target[dots..].replace('.', "/");
        bases.insert(normalize_join(&base, &module));
    }
    expand(bases, extensions(&item.language))
}

fn expand(bases: BTreeSet<String>, extensions: &[&str]) -> BTreeSet<String> {
    let mut result = BTreeSet::new();
    for base in bases {
        result.insert(base.clone());
        if Path::new(&base).extension().is_none() {
            for extension in extensions {
                result.insert(format!("{base}.{extension}"));
                result.insert(format!("{base}/index.{extension}"));
                result.insert(format!("{base}/mod.{extension}"));
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
