use weavatrix_graph::{SourcePosition, SourceSpan};

#[derive(Debug, Clone)]
pub(super) struct StringSite {
    pub pointer: String,
    pub decoded: String,
    pub raw_start: usize,
    pub raw_end: usize,
}

/// Walks raw JSON and records every string with byte offsets and JSON Pointer.
#[must_use]
pub(super) fn string_sites(raw: &str) -> Vec<StringSite> {
    let bytes = raw.as_bytes();
    let mut index = 0;
    let mut sites = Vec::new();
    walk_value(bytes, &mut index, String::new(), &mut sites);
    sites
}

#[must_use]
pub(super) fn span_for(path: &str, raw: &str, start: usize, end: usize) -> SourceSpan {
    let start = position(raw, start.min(raw.len()));
    let end = position(raw, end.min(raw.len()).max(start_offset(raw, start)));
    SourceSpan::new(path, start, end)
}

#[must_use]
pub(super) fn file_span(path: &str) -> SourceSpan {
    SourceSpan::new(
        path,
        SourcePosition::new(1, 1),
        SourcePosition::new(1, 2),
    )
}

/// Maps a decoded-character range onto the raw JSON string contents.
#[must_use]
pub(super) fn map_decoded_range(
    raw_inner: &str,
    decoded: &str,
    decoded_start: usize,
    decoded_end: usize,
) -> (usize, usize) {
    let mut raw_chars = raw_inner.char_indices().peekable();
    let mut decoded_index = 0;
    let mut raw_start = 0;
    let mut raw_end = raw_inner.len();
    let mut started = false;
    while decoded_index < decoded.chars().count() {
        let Some((offset, _)) = raw_chars.next() else {
            break;
        };
        if !started && decoded_index == decoded_start {
            raw_start = offset;
            started = true;
        }
        if raw_inner.as_bytes().get(offset) == Some(&b'\\') {
            raw_chars.next();
        }
        decoded_index += 1;
        if started && decoded_index == decoded_end {
            raw_end = raw_chars.peek().map_or(raw_inner.len(), |(next, _)| *next);
            break;
        }
    }
    (raw_start, raw_end)
}

fn start_offset(raw: &str, position: SourcePosition) -> usize {
    let line = usize::try_from(position.line.saturating_sub(1)).unwrap_or(0);
    raw.split_inclusive('\n').take(line).map(str::len).sum()
}

fn position(raw: &str, offset: usize) -> SourcePosition {
    let prefix = raw.get(..offset).unwrap_or(raw);
    let line = u32::try_from(prefix.bytes().filter(|byte| *byte == b'\n').count() + 1)
        .unwrap_or(u32::MAX);
    let column = u32::try_from(prefix.rsplit('\n').next().map_or(0, |row| row.chars().count()) + 1)
        .unwrap_or(u32::MAX);
    SourcePosition::new(line, column)
}

fn walk_value(bytes: &[u8], index: &mut usize, pointer: String, sites: &mut Vec<StringSite>) {
    skip_ws(bytes, index);
    match bytes.get(*index) {
        Some(b'{') => walk_object(bytes, index, &pointer, sites),
        Some(b'[') => walk_array(bytes, index, &pointer, sites),
        Some(b'"') => {
            if let Some(site) = read_string(bytes, index, pointer) {
                sites.push(site);
            }
        }
        Some(b't' | b'f' | b'n') => skip_literal(bytes, index),
        Some(b'-') | Some(b'0'..=b'9') => skip_number(bytes, index),
        _ => {}
    }
}

fn walk_object(bytes: &[u8], index: &mut usize, pointer: &str, sites: &mut Vec<StringSite>) {
    *index += 1;
    loop {
        skip_ws(bytes, index);
        if bytes.get(*index) == Some(&b'}') {
            *index += 1;
            return;
        }
        let Some(key) = read_string(bytes, index, String::new()) else {
            return;
        };
        skip_ws(bytes, index);
        if bytes.get(*index) == Some(&b':') {
            *index += 1;
        }
        let child = format!("{pointer}/{}", escape_pointer(&key.decoded));
        walk_value(bytes, index, child, sites);
        skip_ws(bytes, index);
        if bytes.get(*index) == Some(&b',') {
            *index += 1;
        }
    }
}

fn walk_array(bytes: &[u8], index: &mut usize, pointer: &str, sites: &mut Vec<StringSite>) {
    *index += 1;
    let mut ordinal = 0;
    loop {
        skip_ws(bytes, index);
        if bytes.get(*index) == Some(&b']') {
            *index += 1;
            return;
        }
        walk_value(bytes, index, format!("{pointer}/{ordinal}"), sites);
        ordinal += 1;
        skip_ws(bytes, index);
        if bytes.get(*index) == Some(&b',') {
            *index += 1;
        }
    }
}

fn read_string(bytes: &[u8], index: &mut usize, pointer: String) -> Option<StringSite> {
    if bytes.get(*index) != Some(&b'"') {
        return None;
    }
    let start = *index;
    *index += 1;
    let mut decoded = String::new();
    let mut escaped = false;
    while let Some(byte) = bytes.get(*index) {
        *index += 1;
        if escaped {
            decoded.push(escape_char(*byte));
            escaped = false;
            continue;
        }
        match byte {
            b'\\' => escaped = true,
            b'"' => {
                return Some(StringSite {
                    pointer,
                    decoded,
                    raw_start: start,
                    raw_end: *index,
                });
            }
            _ => decoded.push(char::from(*byte)),
        }
    }
    None
}

fn escape_char(byte: u8) -> char {
    match byte {
        b'n' => '\n',
        b'r' => '\r',
        b't' => '\t',
        other => char::from(other),
    }
}

fn escape_pointer(value: &str) -> String {
    value.replace('~', "~0").replace('/', "~1")
}

fn skip_ws(bytes: &[u8], index: &mut usize) {
    while bytes
        .get(*index)
        .is_some_and(|byte| byte.is_ascii_whitespace())
    {
        *index += 1;
    }
}

fn skip_literal(bytes: &[u8], index: &mut usize) {
    while bytes.get(*index).is_some_and(u8::is_ascii_alphabetic) {
        *index += 1;
    }
}

fn skip_number(bytes: &[u8], index: &mut usize) {
    while bytes
        .get(*index)
        .is_some_and(|byte| matches!(byte, b'0'..=b'9' | b'+' | b'-' | b'.' | b'e' | b'E'))
    {
        *index += 1;
    }
}
