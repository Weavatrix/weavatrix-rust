use super::super::paths::is_non_product;
use super::Source;
use crate::engine::RepositoryState;
use crate::operations::syntax::matching_delimiter;
use std::collections::BTreeSet;
use std::fs;
use std::path::Path;
use weavatrix_graph::NodeKind;

pub(super) fn runtime_code(path: &str, source: &str) -> String {
    let Some(extension) = Path::new(path).extension().and_then(|value| value.to_str()) else {
        return source.to_owned();
    };
    let Some(language) = weavatrix_parse::Language::from_extension(extension) else {
        return source.to_owned();
    };
    let mut code = source.as_bytes().to_vec();
    for token in weavatrix_parse::tokenize(source, language) {
        if matches!(
            token.kind,
            weavatrix_parse::TokenKind::String
                | weavatrix_parse::TokenKind::Regex
                | weavatrix_parse::TokenKind::LineComment
                | weavatrix_parse::TokenKind::BlockComment
                | weavatrix_parse::TokenKind::Unterminated
        ) {
            // Blank the contents but keep every line break: callers zip this
            // text line by line against the original, and a multi-line string
            // or block comment collapsed into one line shifts every later
            // finding onto the wrong source line.
            for byte in &mut code[token.start..token.end] {
                if !matches!(*byte, b'\n' | b'\r') {
                    *byte = b' ';
                }
            }
        }
    }
    String::from_utf8(code).unwrap_or_else(|_| source.to_owned())
}

pub(super) fn async_context_lines(path: &str, language: &str, source: &str) -> BTreeSet<usize> {
    if language == "python" {
        return python_async_lines(source);
    }
    let Some(extension) = Path::new(path).extension().and_then(|value| value.to_str()) else {
        return BTreeSet::new();
    };
    let Some(language) = weavatrix_parse::Language::from_extension(extension) else {
        return BTreeSet::new();
    };
    if !matches!(
        language,
        weavatrix_parse::Language::JavaScript
            | weavatrix_parse::Language::TypeScript
            | weavatrix_parse::Language::Rust
    ) {
        return BTreeSet::new();
    }
    brace_async_lines(source, language)
}

fn brace_async_lines(source: &str, language: weavatrix_parse::Language) -> BTreeSet<usize> {
    let tokens = weavatrix_parse::tokenize_lite(source, language);
    let mut lines = BTreeSet::new();
    let mut index = 0_usize;
    while index < tokens.len() {
        if tokens[index].text(source) != "async" {
            index += 1;
            continue;
        }
        lines.insert(tokens[index].line as usize);
        let limit = (index + 128).min(tokens.len());
        let Some(open) = (index + 1..limit)
            .take_while(|candidate| tokens[*candidate].text(source) != ";")
            .find(|candidate| tokens[*candidate].text(source) == "{")
        else {
            index += 1;
            continue;
        };
        let Some(close) = matching_delimiter(&tokens, source, open, ("{", "}")) else {
            index += 1;
            continue;
        };
        lines.extend(tokens[index].line as usize..=tokens[close].line as usize);
        index = close + 1;
    }
    lines
}

fn python_async_lines(source: &str) -> BTreeSet<usize> {
    let mut lines = BTreeSet::new();
    let mut scopes = Vec::<usize>::new();
    for (offset, line) in source.lines().enumerate() {
        let trimmed = line.trim_start();
        if trimmed.is_empty() {
            continue;
        }
        let indent = line.len().saturating_sub(trimmed.len());
        while scopes.last().is_some_and(|scope| indent <= *scope) {
            scopes.pop();
        }
        if !scopes.is_empty() {
            lines.insert(offset + 1);
        }
        if trimmed.starts_with("async def ") {
            lines.insert(offset + 1);
            scopes.push(indent);
        }
    }
    lines
}

/// Lines compiled only under `cfg(test)`.
pub(in crate::operations::health) fn rust_cfg_test_lines(source: &str) -> BTreeSet<usize> {
    let tokens = weavatrix_parse::tokenize_lite(source, weavatrix_parse::Language::Rust);
    let mut ignored = BTreeSet::new();
    let mut index = 0_usize;
    while index + 2 < tokens.len() {
        if tokens[index].text(source) != "#"
            || tokens[index + 1].text(source) != "["
            || tokens[index + 2].text(source) != "cfg"
        {
            index += 1;
            continue;
        }
        let Some(attribute_end) =
            (index + 3..tokens.len()).find(|candidate| tokens[*candidate].text(source) == "]")
        else {
            break;
        };
        if !(index + 3..attribute_end).any(|candidate| tokens[candidate].text(source) == "test") {
            index = attribute_end + 1;
            continue;
        }
        let Some(open) = (attribute_end + 1..tokens.len())
            .find(|candidate| tokens[*candidate].text(source) == "{")
        else {
            break;
        };
        let Some(close) = matching_delimiter(&tokens, source, open, ("{", "}")) else {
            break;
        };
        ignored.extend(tokens[index].line as usize..=tokens[close].line as usize);
        index = close + 1;
    }
    ignored
}

/// Production source text of the analyzed worktree.
pub(in crate::operations::health) fn product_sources(state: &RepositoryState) -> Vec<Source> {
    state
        .graph()
        .nodes()
        .iter()
        .filter(|node| node.kind == NodeKind::File)
        .filter_map(|node| {
            let language = node.language.clone()?;
            if is_non_product(&node.label) {
                return None;
            }
            let text = fs::read_to_string(state.root().join(&node.label)).ok()?;
            Some((node.label.clone(), language, text))
        })
        .collect()
}
