#[must_use]
pub(super) fn mask(raw: &str) -> Vec<bool> {
    let mut out = vec![true; raw.len()];
    let bytes = raw.as_bytes();
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'/' && bytes.get(index + 1) == Some(&b'/') {
            fill(&mut out, index, line_end(bytes, index), false);
            index = line_end(bytes, index);
            continue;
        }
        if bytes[index] == b'/' && bytes.get(index + 1) == Some(&b'*') {
            let end = block_end(bytes, index + 2);
            fill(&mut out, index, end, false);
            index = end;
            continue;
        }
        if matches!(bytes[index], b'\'' | b'"' | b'`') {
            let end = string_end(bytes, index);
            fill(&mut out, index, end, false);
            index = end;
            continue;
        }
        index += 1;
    }
    out
}

#[must_use]
pub(super) fn at(mask: &[bool], index: usize) -> bool {
    mask.get(index).copied().unwrap_or(false)
}

#[must_use]
pub(super) fn ident_start(raw: &str, index: usize) -> bool {
    if index == 0 {
        return true;
    }
    !raw.as_bytes()
        .get(index - 1)
        .is_some_and(|byte| byte.is_ascii_alphanumeric() || *byte == b'_')
}

fn fill(mask: &mut [bool], start: usize, end: usize, value: bool) {
    let end = end.min(mask.len());
    if start < end {
        mask[start..end].fill(value);
    }
}

fn line_end(bytes: &[u8], start: usize) -> usize {
    bytes
        .iter()
        .enumerate()
        .skip(start)
        .find(|(_, byte)| **byte == b'\n')
        .map_or(bytes.len(), |(index, _)| index + 1)
}

fn block_end(bytes: &[u8], start: usize) -> usize {
    let mut index = start;
    while index + 1 < bytes.len() {
        if bytes[index] == b'*' && bytes[index + 1] == b'/' {
            return index + 2;
        }
        index += 1;
    }
    bytes.len()
}

fn string_end(bytes: &[u8], start: usize) -> usize {
    let quote = bytes[start];
    let mut index = start + 1;
    while index < bytes.len() {
        if bytes[index] == b'\\' {
            index = index.saturating_add(2);
            continue;
        }
        if bytes[index] == quote {
            return index + 1;
        }
        if quote != b'`' && bytes[index] == b'\n' {
            return index;
        }
        index += 1;
    }
    bytes.len()
}
