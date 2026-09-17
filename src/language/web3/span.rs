use weavatrix_graph::{SourcePosition, SourceSpan};

#[must_use]
pub(super) fn file_span(path: &str) -> SourceSpan {
    SourceSpan::new(path, SourcePosition::new(1, 1), SourcePosition::new(1, 2))
}

#[must_use]
pub(super) fn span_for(path: &str, raw: &str, start: usize, end: usize) -> SourceSpan {
    let start = start.min(raw.len());
    let end = end.min(raw.len()).max(start.saturating_add(1));
    SourceSpan::new(path, position(raw, start), position(raw, end))
}

#[must_use]
pub(super) fn find_name(path: &str, raw: &str, name: &str) -> SourceSpan {
    let needle = format!("\"{name}\"");
    if let Some(at) = raw.find(&needle) {
        return span_for(path, raw, at, at + needle.len());
    }
    let single = format!("'{name}'");
    if let Some(at) = raw.find(&single) {
        return span_for(path, raw, at, at + single.len());
    }
    file_span(path)
}

#[must_use]
fn position(raw: &str, offset: usize) -> SourcePosition {
    let mut line = 1_u32;
    let mut column = 1_u32;
    for (index, ch) in raw.char_indices() {
        if index >= offset {
            break;
        }
        if ch == '\n' {
            line = line.saturating_add(1);
            column = 1;
        } else {
            column = column.saturating_add(1);
        }
    }
    SourcePosition::new(line, column)
}
