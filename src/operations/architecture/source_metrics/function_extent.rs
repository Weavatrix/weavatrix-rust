pub(super) fn lines(source: &str, start_line: u32, language: Option<&str>) -> u64 {
    if language == Some("python") {
        return indented_lines(source, start_line);
    }
    let Some(start) = line_offset(source, start_line) else {
        return 1;
    };
    let masked = mask_non_code(source);
    let Some(open) = body_open(&masked, start) else {
        return 1;
    };
    let mut depth = 0_u64;
    for (offset, byte) in masked[open..].iter().copied().enumerate() {
        if byte == b'{' {
            depth += 1;
        } else if byte == b'}' {
            depth = depth.saturating_sub(1);
            if depth == 0 {
                let end = open + offset;
                return u64::from(line_at(&masked, end).saturating_sub(start_line) + 1);
            }
        }
    }
    1
}

fn body_open(source: &[u8], start: usize) -> Option<usize> {
    let mut parentheses = 0_u32;
    let mut brackets = 0_u32;
    for (offset, byte) in source[start..].iter().copied().enumerate() {
        match byte {
            b'(' => parentheses += 1,
            b')' => parentheses = parentheses.saturating_sub(1),
            b'[' => brackets += 1,
            b']' => brackets = brackets.saturating_sub(1),
            b';' if parentheses == 0 && brackets == 0 => return None,
            b'{' if parentheses == 0 && brackets == 0 => return Some(start + offset),
            _ => {}
        }
    }
    None
}

fn indented_lines(source: &str, start_line: u32) -> u64 {
    let lines = source.lines().collect::<Vec<_>>();
    let Some(start) = usize::try_from(start_line)
        .ok()
        .and_then(|line| line.checked_sub(1))
    else {
        return 1;
    };
    let Some(first) = lines.get(start) else {
        return 1;
    };
    let indent = indentation(first);
    let mut end = start;
    for (index, line) in lines.iter().enumerate().skip(start + 1) {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            end = index;
            continue;
        }
        if indentation(line) <= indent {
            break;
        }
        end = index;
    }
    u64::try_from(end - start + 1).unwrap_or(u64::MAX)
}

fn indentation(line: &str) -> usize {
    line.chars()
        .take_while(|character| character.is_whitespace())
        .map(|character| if character == '\t' { 4 } else { 1 })
        .sum()
}

fn line_offset(source: &str, line: u32) -> Option<usize> {
    if line == 0 {
        return None;
    }
    if line == 1 {
        return Some(0);
    }
    let target = usize::try_from(line - 1).ok()?;
    source
        .match_indices('\n')
        .nth(target - 1)
        .map(|(offset, _)| offset + 1)
}

fn line_at(source: &[u8], offset: usize) -> u32 {
    let lines = source[..offset].split(|byte| *byte == b'\n').count();
    u32::try_from(lines).unwrap_or(u32::MAX)
}

fn mask_non_code(source: &str) -> Vec<u8> {
    let bytes = source.as_bytes();
    let mut output = bytes.to_vec();
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index..].starts_with(b"//") {
            index = mask_line(bytes, &mut output, index);
        } else if bytes[index..].starts_with(b"/*") {
            index = mask_block(bytes, &mut output, index);
        } else if let Some((content, hashes)) = raw_string(bytes, index) {
            index = mask_raw(bytes, &mut output, content, hashes);
        } else if matches!(bytes[index], b'"' | b'`')
            || (bytes[index] == b'\'' && looks_like_character(bytes, index))
        {
            index = mask_quoted(bytes, &mut output, index, bytes[index]);
        } else {
            index += 1;
        }
    }
    output
}

fn mask_line(bytes: &[u8], output: &mut [u8], mut index: usize) -> usize {
    while index < bytes.len() && bytes[index] != b'\n' {
        output[index] = b' ';
        index += 1;
    }
    index
}

fn mask_block(bytes: &[u8], output: &mut [u8], mut index: usize) -> usize {
    let mut depth = 0_u32;
    while index < bytes.len() {
        if bytes[index..].starts_with(b"/*") {
            depth += 1;
            blank(output, index, 2);
            index += 2;
        } else if bytes[index..].starts_with(b"*/") {
            depth = depth.saturating_sub(1);
            blank(output, index, 2);
            index += 2;
            if depth == 0 {
                break;
            }
        } else {
            blank(output, index, 1);
            index += 1;
        }
    }
    index
}

fn raw_string(bytes: &[u8], index: usize) -> Option<(usize, usize)> {
    let mut cursor = index;
    if bytes.get(cursor) == Some(&b'b') {
        cursor += 1;
    }
    if bytes.get(cursor) != Some(&b'r') {
        return None;
    }
    cursor += 1;
    let start = cursor;
    while bytes.get(cursor) == Some(&b'#') {
        cursor += 1;
    }
    (bytes.get(cursor) == Some(&b'"')).then_some((cursor + 1, cursor - start))
}

fn mask_raw(bytes: &[u8], output: &mut [u8], mut index: usize, hashes: usize) -> usize {
    while index < bytes.len() {
        blank(output, index, 1);
        if bytes[index] == b'"' && closing_hashes(bytes, index + 1, hashes) {
            blank(output, index + 1, hashes);
            return index + 1 + hashes;
        }
        index += 1;
    }
    index
}

fn closing_hashes(bytes: &[u8], start: usize, hashes: usize) -> bool {
    bytes
        .get(start..start + hashes)
        .is_some_and(|suffix| suffix.iter().all(|byte| *byte == b'#'))
}

fn mask_quoted(bytes: &[u8], output: &mut [u8], mut index: usize, quote: u8) -> usize {
    blank(output, index, 1);
    index += 1;
    while index < bytes.len() {
        blank(output, index, 1);
        if bytes[index] == b'\\' {
            blank(output, index + 1, 1);
            index += 2;
        } else if bytes[index] == quote {
            return index + 1;
        } else {
            index += 1;
        }
    }
    index
}

fn looks_like_character(bytes: &[u8], index: usize) -> bool {
    bytes
        .iter()
        .skip(index + 1)
        .take(12)
        .take_while(|byte| **byte != b'\n')
        .any(|byte| *byte == b'\'')
}

fn blank(output: &mut [u8], index: usize, length: usize) {
    for byte in output.iter_mut().skip(index).take(length) {
        if *byte != b'\n' && *byte != b'\r' {
            *byte = b' ';
        }
    }
}

#[cfg(test)]
mod tests {
    use super::lines;

    #[test]
    fn braces_in_comments_and_strings_do_not_change_the_extent() {
        let source = "fn measured() {\n\
                      let raw = r#\"}\"#;\n\
                      // }\n\
                      let value = 1;\n\
                      }\n";
        assert_eq!(lines(source, 1, Some("rust")), 5);
    }

    #[test]
    fn python_extent_uses_the_declaration_indentation() {
        let source = "def measured():\n    value = 1\n    return value\n\noutside = 2\n";
        assert_eq!(lines(source, 1, Some("python")), 4);
    }

    #[test]
    fn declarations_without_bodies_are_one_line() {
        assert_eq!(lines("fn declaration();\n", 1, Some("rust")), 1);
    }
}
