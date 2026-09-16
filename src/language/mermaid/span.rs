use weavatrix_graph::{SourcePosition, SourceSpan};

#[must_use]
pub(super) fn span_for(path: &str, raw: &str, start: usize, end: usize) -> SourceSpan {
    let start = start.min(raw.len());
    let end = end.min(raw.len()).max(start);
    SourceSpan::new(path, position(raw, start), position(raw, end.max(start + 1)))
}

#[must_use]
pub(super) fn file_span(path: &str) -> SourceSpan {
    SourceSpan::new(path, SourcePosition::new(1, 1), SourcePosition::new(1, 2))
}

fn position(raw: &str, offset: usize) -> SourcePosition {
    let mut line = 1_u32;
    let mut column = 1_u32;
    for character in raw[..offset.min(raw.len())].chars() {
        if character == '\n' {
            line = line.saturating_add(1);
            column = 1;
        } else if character != '\r' {
            column = column.saturating_add(1);
        }
    }
    SourcePosition::new(line.max(1), column.max(1))
}
