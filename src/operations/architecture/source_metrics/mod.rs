//! Source-level budget measurements for architecture verification.

use super::contract::component_for;
use crate::engine::RepositoryState;
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Component, Path};
use weavatrix_graph::NodeKind;

mod function_extent;

pub(super) struct SourceMetrics {
    pub files: Vec<FileMetric>,
    pub functions: Vec<FunctionMetric>,
}

pub(super) struct FileMetric {
    pub path: String,
    pub lines: u64,
}

pub(super) struct FunctionMetric {
    pub file: String,
    pub name: String,
    pub kind: String,
    pub start_line: u32,
    pub lines: u64,
}

pub(super) fn collect(
    state: &RepositoryState,
    contract: &blazingly_json::Value,
    include_functions: bool,
) -> Result<SourceMetrics, String> {
    let mut sources = BTreeMap::<String, String>::new();
    let mut files = Vec::new();
    for node in state.graph().nodes() {
        if node.kind != NodeKind::File || component_for(contract, &node.label).is_none() {
            continue;
        }
        let source = read_repository_file(state.root(), &node.label)?;
        files.push(FileMetric {
            path: node.label.clone(),
            lines: physical_lines(&source),
        });
        sources.insert(node.label.clone(), source);
    }
    files.sort_by(|left, right| left.path.cmp(&right.path));

    let mut functions = Vec::new();
    let mut seen = BTreeSet::new();
    if include_functions {
        for node in state.graph().nodes() {
            if !matches!(node.kind, NodeKind::Function | NodeKind::Method) {
                continue;
            }
            let Some(span) = node.span.as_ref() else {
                continue;
            };
            let Some(source) = sources.get(&span.file) else {
                continue;
            };
            let kind = node.kind.as_str().to_owned();
            let key = (
                span.file.clone(),
                span.start.line,
                kind.clone(),
                node.label.clone(),
            );
            if !seen.insert(key) {
                continue;
            }
            functions.push(FunctionMetric {
                file: span.file.clone(),
                name: node.label.clone(),
                kind,
                start_line: span.start.line,
                lines: function_extent::lines(source, span.start.line, node.language.as_deref()),
            });
        }
    }
    functions.sort_by(|left, right| {
        (&left.file, left.start_line, &left.kind, &left.name).cmp(&(
            &right.file,
            right.start_line,
            &right.kind,
            &right.name,
        ))
    });
    Ok(SourceMetrics { files, functions })
}

fn read_repository_file(root: &Path, relative: &str) -> Result<String, String> {
    let relative_path = Path::new(relative);
    if relative_path.is_absolute()
        || relative_path.components().any(|part| {
            matches!(
                part,
                Component::ParentDir | Component::RootDir | Component::Prefix(_)
            )
        })
    {
        return Err(format!(
            "architecture source path escapes repository: {relative}"
        ));
    }
    let canonical_root =
        fs::canonicalize(root).map_err(|error| format!("{}: {error}", root.display()))?;
    let candidate = root.join(relative_path);
    let canonical = fs::canonicalize(&candidate)
        .map_err(|error| format!("{}: {error}", candidate.display()))?;
    if !canonical.starts_with(&canonical_root) {
        return Err(format!(
            "architecture source path escapes repository: {relative}"
        ));
    }
    fs::read_to_string(&canonical).map_err(|error| format!("{}: {error}", canonical.display()))
}

/// Physical extent in lines of the declaration starting at `start_line`,
/// measured from source. Shared with reviews that must not treat a
/// single-line declaration span as a one-line function.
pub(in crate::operations) fn function_lines(
    source: &str,
    start_line: u32,
    language: Option<&str>,
) -> u64 {
    function_extent::lines(source, start_line, language)
}

fn physical_lines(source: &str) -> u64 {
    if source.is_empty() {
        return 0;
    }
    let newlines = source.bytes().filter(|byte| *byte == b'\n').count();
    u64::try_from(newlines + usize::from(!source.ends_with('\n'))).unwrap_or(u64::MAX)
}
