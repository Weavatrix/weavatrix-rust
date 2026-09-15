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
    SourceSpan::new(path, SourcePosition::new(1, 1), SourcePosition::new(1, 2))
}

/// Maps a decoded-character range onto the raw JSON string contents (escapes included).
#[must_use]
pub(super) fn map_decoded_range(
    raw_inner: &str,
    decoded: &str,
    decoded_start: usize,
    decoded_end: usize,
) -> (usize, usize) {
    let raw = raw_inner.as_bytes();
    let mut raw_index = 0;
    let mut decoded_index = 0;
    let mut raw_start = 0;
    let mut raw_end = raw_inner.len();
    let mut started = false;
    for character in decoded.chars() {
        if !started && decoded_index == decoded_start {
            raw_start = raw_index;
            started = true;
        }
        raw_index = skip_raw_char(raw, raw_index, character);
        decoded_index += 1;
        if started && decoded_index == decoded_end {
            raw_end = raw_index;
            break;
        }
    }
    (raw_start, raw_end)
}

fn skip_raw_char(raw: &[u8], index: usize, decoded: char) -> usize {
    if raw.get(index) == Some(&b'\\') {
        if raw.get(index + 1) == Some(&b'u') {
            let after = index.saturating_add(6);
            if is_high_surrogate(decoded)
                && raw.get(after) == Some(&b'\\')
                && raw.get(after + 1) == Some(&b'u')
            {
                return after.saturating_add(6);
            }
            return after;
        }
        return index.saturating_add(2);
    }
    index.saturating_add(decoded.len_utf8())
}

fn is_high_surrogate(character: char) -> bool {
    let value = u32::from(character);
    (0x1_0000..=0x10_FFFF).contains(&value)
}

fn start_offset(raw: &str, position: SourcePosition) -> usize {
    let line = usize::try_from(position.line.saturating_sub(1)).unwrap_or(0);
    raw.split_inclusive('\n').take(line).map(str::len).sum()
}

fn position(raw: &str, offset: usize) -> SourcePosition {
    let prefix = raw.get(..offset).unwrap_or(raw);
    let line =
        u32::try_from(prefix.bytes().filter(|byte| *byte == b'\n').count() + 1).unwrap_or(u32::MAX);
    let column = u32::try_from(
        prefix
            .rsplit('\n')
            .next()
            .map_or(0, |row| row.chars().count())
            + 1,
    )
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
        Some(b'-' | b'0'..=b'9') => skip_number(bytes, index),
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
            decoded.push(decode_escape(bytes, index, *byte)?);
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
            _ => {
                *index -= 1;
                decoded.push(next_utf8(bytes, index)?);
            }
        }
    }
    None
}

fn next_utf8(bytes: &[u8], index: &mut usize) -> Option<char> {
    let first = *bytes.get(*index)?;
    let width = match first {
        0x00..=0x7F => 1,
        0xC2..=0xDF => 2,
        0xE0..=0xEF => 3,
        0xF0..=0xF4 => 4,
        _ => return None,
    };
    let slice = bytes.get(*index..index.saturating_add(width))?;
    let character = std::str::from_utf8(slice).ok()?.chars().next()?;
    *index += width;
    Some(character)
}

fn decode_escape(bytes: &[u8], index: &mut usize, marker: u8) -> Option<char> {
    match marker {
        b'n' => Some('\n'),
        b'r' => Some('\r'),
        b't' => Some('\t'),
        b'b' => Some('\u{0008}'),
        b'f' => Some('\u{000c}'),
        b'u' => decode_unicode(bytes, index),
        other => Some(char::from(other)),
    }
}

fn decode_unicode(bytes: &[u8], index: &mut usize) -> Option<char> {
    let high = read_hex4(bytes, index)?;
    if (0xD800..=0xDBFF).contains(&high) {
        if bytes.get(*index) == Some(&b'\\') && bytes.get(*index + 1) == Some(&b'u') {
            *index += 2;
            let low = read_hex4(bytes, index)?;
            if (0xDC00..=0xDFFF).contains(&low) {
                let code = 0x1_0000 + (((high - 0xD800) << 10) | (low - 0xDC00));
                return char::from_u32(code);
            }
        }
        return None;
    }
    char::from_u32(high)
}

fn read_hex4(bytes: &[u8], index: &mut usize) -> Option<u32> {
    let mut value = 0_u32;
    for _ in 0..4 {
        let digit = bytes.get(*index)?;
        *index += 1;
        value = (value << 4) | hex_digit(*digit)?;
    }
    Some(value)
}

fn hex_digit(byte: u8) -> Option<u32> {
    match byte {
        b'0'..=b'9' => Some(u32::from(byte - b'0')),
        b'a'..=b'f' => Some(u32::from(byte - b'a' + 10)),
        b'A'..=b'F' => Some(u32::from(byte - b'A' + 10)),
        _ => None,
    }
}

fn escape_pointer(value: &str) -> String {
    value.replace('~', "~0").replace('/', "~1")
}

fn skip_ws(bytes: &[u8], index: &mut usize) {
    while bytes.get(*index).is_some_and(u8::is_ascii_whitespace) {
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

#[cfg(test)]
mod tests {
    use super::{map_decoded_range, string_sites};

    #[test]
    fn escaped_unicode_and_quotes_keep_raw_ranges() {
        let raw = r#"{"jsCode":"const x = \"hello\";\nreturn \"\u0040\";"}"#;
        let site = string_sites(raw)
            .into_iter()
            .find(|site| site.pointer == "/jsCode")
            .unwrap();
        assert!(site.decoded.contains('@'));
        let inner = &raw[site.raw_start + 1..site.raw_end - 1];
        let at = site.decoded.find('@').unwrap();
        let (start, end) = map_decoded_range(inner, &site.decoded, at, at + 1);
        assert_eq!(&inner[start..end], r"\u0040");
    }
}
