use blazingly_json::{Value, json};

pub(super) fn render_search_match(
    item: weavatrix_search::SearchMatch,
    line_limit: Option<usize>,
) -> Value {
    let original_len = item.line.len();
    let (start, end) = if let Some(limit) = line_limit.filter(|limit| original_len > *limit) {
        let anchor = item
            .spans
            .first()
            .map_or(0, |span| span.start.min(original_len));
        let mut start = anchor.saturating_sub(80);
        while start > 0 && !item.line.is_char_boundary(start) {
            start -= 1;
        }
        let mut end = start.saturating_add(limit).min(original_len);
        while end > start && !item.line.is_char_boundary(end) {
            end -= 1;
        }
        (start, end)
    } else {
        (0, original_len)
    };
    let spans = item
        .spans
        .into_iter()
        .filter(|span| span.start >= start && span.end <= end)
        .map(|span| {
            json!({
                "pattern": span.pattern_index,
                "start": span.start - start,
                "end": span.end - start,
            })
        })
        .collect::<Vec<_>>();
    json!({
        "path": item.path,
        "line": item.line_number,
        "end_line": item.end_line_number,
        "text": &item.line[start..end],
        "text_truncated": start > 0 || end < original_len,
        "encoding": item.encoding,
        "spans": spans,
    })
}
