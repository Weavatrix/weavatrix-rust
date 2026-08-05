//! Clone line spans a caller can verify byte for byte.
//!
//! A token window starts and ends wherever the matched token run starts and
//! ends, which is regularly mid-line. Reporting the raw first and last token
//! lines made `strict_equal` evidence unverifiable: the boundary lines carry
//! text the matcher never compared, so diffing the reported ranges shows
//! differences the report appears to deny. Every reported location is
//! therefore shrunk to the lines the match covers completely.

use std::collections::HashMap;
use std::path::Path;
use weavatrix_clone::{CloneLocation, ClonePair, SourceSpan};

#[derive(Default)]
pub(super) struct SourceCache {
    files: HashMap<String, Option<String>>,
}

impl SourceCache {
    fn text(&mut self, root: &Path, path: &str) -> Option<&str> {
        self.files
            .entry(path.to_owned())
            .or_insert_with(|| std::fs::read_to_string(root.join(path)).ok())
            .as_deref()
    }
}

pub(super) fn trim_pairs(root: &Path, pairs: &mut [ClonePair], cache: &mut SourceCache) {
    for pair in pairs {
        trim_location(root, &mut pair.left, cache);
        trim_location(root, &mut pair.right, cache);
    }
}

fn trim_location(root: &Path, location: &mut CloneLocation, cache: &mut SourceCache) {
    if let Some(text) = cache.text(root, &location.path)
        && let Some(span) = whole_lines(text, location.span)
    {
        location.span = span;
    }
}

/// Shrinks `span` to the lines the match covers completely.
///
/// Returns `None` when no complete line survives, when the byte offsets do not
/// land on character boundaries, or when the file no longer matches the span:
/// the raw token span then stays the only honest answer.
fn whole_lines(text: &str, span: SourceSpan) -> Option<SourceSpan> {
    let start_byte = span.start_byte.min(text.len());
    let end_byte = span.end_byte.clamp(start_byte, text.len());
    let (start_byte, start_line) = first_whole_line(text, start_byte, span.start_line)?;
    let (end_byte, end_line) = last_whole_line(text, end_byte, span.end_line)?;
    (start_line <= end_line && start_byte <= end_byte).then_some(SourceSpan {
        start_byte,
        end_byte,
        start_line,
        end_line,
    })
}

/// Widens the start to its own line when only indentation precedes the match,
/// and otherwise drops the partially matched first line.
fn first_whole_line(text: &str, start_byte: usize, start_line: u32) -> Option<(usize, u32)> {
    let line_start = text
        .get(..start_byte)?
        .rfind('\n')
        .map_or(0, |index| index + 1);
    if text.get(line_start..start_byte)?.trim().is_empty() {
        return Some((line_start, start_line));
    }
    let newline = text.get(start_byte..)?.find('\n')?;
    Some((
        start_byte.saturating_add(newline).saturating_add(1),
        start_line.saturating_add(1),
    ))
}

/// Widens the end to its own line end when only trailing whitespace follows the
/// match, and otherwise drops the partially matched last line.
fn last_whole_line(text: &str, end_byte: usize, end_line: u32) -> Option<(usize, u32)> {
    let line_end = text
        .get(end_byte..)?
        .find('\n')
        .map_or(text.len(), |index| end_byte.saturating_add(index));
    if text.get(end_byte..line_end)?.trim().is_empty() {
        return Some((line_end, end_line));
    }
    let newline = text.get(..end_byte)?.rfind('\n')?;
    Some((newline, end_line.checked_sub(1)?))
}

#[cfg(test)]
mod tests {
    use super::whole_lines;
    use weavatrix_clone::SourceSpan;

    const SOURCE: &str = "alpha, beta,\ngamma,\ndelta, epsilon\n";

    fn span(start_byte: usize, end_byte: usize, start_line: u32, end_line: u32) -> SourceSpan {
        SourceSpan {
            start_byte,
            end_byte,
            start_line,
            end_line,
        }
    }

    #[test]
    fn partially_covered_boundary_lines_are_dropped() {
        // Starts inside line 1 (after `alpha,`) and ends inside line 3.
        let trimmed = whole_lines(SOURCE, span(7, 26, 1, 3)).unwrap();
        assert_eq!(trimmed.start_line, 2);
        assert_eq!(trimmed.end_line, 2);
        assert_eq!(&SOURCE[trimmed.start_byte..trimmed.end_byte], "gamma,");
    }

    #[test]
    fn indentation_and_trailing_whitespace_keep_their_lines() {
        let source = "  alpha,\n  beta,  \ngamma\n";
        let trimmed = whole_lines(source, span(2, 16, 1, 2)).unwrap();
        assert_eq!((trimmed.start_line, trimmed.end_line), (1, 2));
        assert_eq!(
            &source[trimmed.start_byte..trimmed.end_byte],
            "  alpha,\n  beta,  "
        );
    }

    #[test]
    fn a_match_inside_a_single_line_keeps_the_raw_span() {
        assert!(whole_lines(SOURCE, span(7, 11, 1, 1)).is_none());
    }
}
