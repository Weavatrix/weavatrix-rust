use weavatrix_graph::{SourcePosition, SourceSpan};

#[must_use]
pub(super) fn span_for(path: &str, raw: &str, start: usize, end: usize) -> SourceSpan {
    let start = previous_boundary(raw, start.min(raw.len()));
    let end = next_boundary(raw, end.min(raw.len()).max(start.saturating_add(1)));
    SourceSpan::new(path, position(raw, start), position(raw, end))
}

#[must_use]
pub(super) fn file_span(path: &str) -> SourceSpan {
    SourceSpan::new(path, SourcePosition::new(1, 1), SourcePosition::new(1, 2))
}

fn position(raw: &str, offset: usize) -> SourcePosition {
    let mut line = 1_u32;
    let mut column = 1_u32;
    for character in raw[..previous_boundary(raw, offset.min(raw.len()))].chars() {
        if character == '\n' {
            line = line.saturating_add(1);
            column = 1;
        } else if character != '\r' {
            column = column.saturating_add(1);
        }
    }
    SourcePosition::new(line.max(1), column.max(1))
}

fn previous_boundary(raw: &str, mut offset: usize) -> usize {
    while !raw.is_char_boundary(offset) {
        offset -= 1;
    }
    offset
}

fn next_boundary(raw: &str, mut offset: usize) -> usize {
    while offset < raw.len() && !raw.is_char_boundary(offset) {
        offset += 1;
    }
    offset
}

#[cfg(test)]
mod tests {
    use super::span_for;

    #[test]
    fn span_offsets_inside_unicode_characters_do_not_panic_or_split_a_character() {
        let raw = "A→Б\n";
        let span = span_for("diagram.mmd", raw, 2, 5);
        assert_eq!(span.start.line, 1);
        assert_eq!(span.start.column, 2);
        assert_eq!(span.end.column, 4);
        let end_of_file = span_for("diagram.mmd", raw, raw.len(), raw.len());
        assert_eq!(end_of_file.end.line, 2);
    }
}
