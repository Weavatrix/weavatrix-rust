//! Opt-in clone evidence for embedded string payloads.
//!
//! The code pass compares tokens, and a string literal is one token however
//! much it carries: an inline SQL statement, a C# or PowerShell template, a
//! shell script pasted into a constant. Such a payload never reaches
//! `min_tokens` on its own, so duplicated templates stay invisible to clone
//! review. `include_strings` lifts every multi-line literal out as its own
//! fragment and tokenizes the content, because there the content is the code.

use crate::engine::RepositoryState;
use weavatrix_clone::{Language, SourceFragment, SourceSpan};
use weavatrix_graph::NodeKind;

/// A literal shorter than this carries a message, not a payload.
const MIN_LINES: usize = 3;

/// Non-overlapping window for a literal too long to compare whole.
///
/// A section shared by two long templates dilutes below the similarity floor
/// when the whole literals are compared - seventy shared lines inside a
/// two-hundred-line template is barely a third of it - so a long literal is
/// compared in blocks instead.
const WINDOW_LINES: usize = 24;

/// Every multi-line string payload in the repository, as comparable fragments.
pub(super) fn fragments(state: &RepositoryState) -> Vec<SourceFragment> {
    let mut fragments = Vec::new();
    for node in state
        .graph()
        .nodes()
        .iter()
        .filter(|node| node.kind == NodeKind::File)
    {
        let path = node.label.as_str();
        let Some(language) = std::path::Path::new(path)
            .extension()
            .and_then(|value| value.to_str())
            .and_then(weavatrix_parse::Language::from_extension)
        else {
            continue;
        };
        let Ok(text) = std::fs::read_to_string(state.root().join(path)) else {
            continue;
        };
        for token in weavatrix_parse::tokenize(&text, language) {
            if token.kind == weavatrix_parse::TokenKind::String {
                collect(&mut fragments, path, &text, &token);
            }
        }
    }
    fragments
}

fn collect(
    fragments: &mut Vec<SourceFragment>,
    path: &str,
    text: &str,
    token: &weavatrix_parse::Token,
) {
    let Some(raw) = text.get(token.start..token.end) else {
        return;
    };
    let Some((offset, payload)) = payload(raw) else {
        return;
    };
    let start = token.start.saturating_add(offset);
    let line = token
        .line
        .saturating_add(line_count(raw.get(..offset).unwrap_or_default()));
    let starts = line_starts(payload);
    if starts.len() < MIN_LINES {
        return;
    }
    if starts.len() <= WINDOW_LINES.saturating_mul(2) {
        push(fragments, path, start, line, payload);
        return;
    }
    for (index, block) in starts.chunks(WINDOW_LINES).enumerate() {
        if block.len() < MIN_LINES {
            break;
        }
        let from = block[0];
        let to = starts
            .get(index.saturating_add(1).saturating_mul(WINDOW_LINES))
            .copied()
            .unwrap_or(payload.len());
        let Some(window) = payload.get(from..to) else {
            break;
        };
        let skipped = u32::try_from(index.saturating_mul(WINDOW_LINES)).unwrap_or(u32::MAX);
        push(
            fragments,
            path,
            start.saturating_add(from),
            line.saturating_add(skipped),
            window.trim_end_matches(['\n', '\r']),
        );
    }
}

/// Byte offset of every line the text opens.
fn line_starts(text: &str) -> Vec<usize> {
    let mut starts = vec![0_usize];
    starts.extend(
        text.match_indices('\n')
            .map(|(index, _)| index.saturating_add(1))
            .filter(|index| *index < text.len()),
    );
    starts
}

fn push(fragments: &mut Vec<SourceFragment>, path: &str, start: usize, line: u32, text: &str) {
    let end = start.saturating_add(text.len());
    if let Ok(fragment) = SourceFragment::new(
        format!("{path}#string:{start}-{end}"),
        path,
        // The payload is foreign text, so no host-language comment or escape
        // rule may be applied to it.
        Language::Text,
        SourceSpan {
            start_byte: start,
            end_byte: end,
            start_line: line,
            end_line: line.saturating_add(line_count(text)),
        },
        text,
    ) {
        fragments.push(fragment);
    }
}

/// The payload of a literal, and its byte offset inside the raw token.
///
/// Delimiters differ per language - `` ` ``, `"""`, `r#"..."#` - and comparing
/// them would keep one template from matching its copy in another host
/// language, which is the case this option exists for.
fn payload(raw: &str) -> Option<(usize, &str)> {
    let quote = raw
        .chars()
        .find(|value| matches!(value, '"' | '\'' | '`'))?;
    let opening = raw.find(quote)?;
    let bytes = raw.as_bytes();
    let quote = u8::try_from(quote).ok()?;
    let tripled = bytes.get(opening.saturating_add(1)) == Some(&quote)
        && bytes.get(opening.saturating_add(2)) == Some(&quote);
    let repeats = if tripled { 3 } else { 1 };
    let start = opening.saturating_add(repeats);
    let end = raw
        .trim_end_matches('#')
        .len()
        .saturating_sub(repeats)
        .max(start);
    raw.get(start..end).map(|payload| (start, payload))
}

fn line_count(text: &str) -> u32 {
    u32::try_from(text.bytes().filter(|byte| *byte == b'\n').count()).unwrap_or(u32::MAX)
}

#[cfg(test)]
mod tests {
    use super::{line_starts, payload};

    #[test]
    fn delimiters_are_stripped_so_a_payload_matches_across_host_languages() {
        let template = payload("`select 1\nfrom t\nwhere x`").unwrap().1;
        let tripled = payload("\"\"\"select 1\nfrom t\nwhere x\"\"\"").unwrap().1;
        let raw = payload("r#\"select 1\nfrom t\nwhere x\"#").unwrap().1;

        assert_eq!(template, "select 1\nfrom t\nwhere x");
        assert_eq!(template, tripled);
        assert_eq!(template, raw);
    }

    #[test]
    fn the_offset_locates_the_payload_inside_the_raw_literal() {
        let raw = "r#\"payload\"#";
        let (offset, text) = payload(raw).unwrap();

        assert_eq!(raw.get(offset..offset + text.len()), Some("payload"));
    }

    #[test]
    fn an_empty_or_absent_literal_is_not_a_panic() {
        assert_eq!(payload("\"\"").unwrap().1, "");
        assert_eq!(payload("``").unwrap().1, "");
        assert!(payload("42").is_none());
    }

    #[test]
    fn a_trailing_newline_does_not_open_a_line() {
        assert_eq!(line_starts("a\nb\n"), vec![0, 2]);
        assert_eq!(line_starts("a\nb"), vec![0, 2]);
        assert_eq!(line_starts(""), vec![0]);
    }
}
