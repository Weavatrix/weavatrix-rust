use super::detect::is_standalone;
use super::model::Region;

#[must_use]
pub(super) fn extract(path: &str, raw: &str) -> Vec<Region> {
    if is_standalone(path) {
        let start = bom_len(raw);
        return vec![Region {
            index: 0,
            start,
            end: raw.len(),
        }];
    }
    fenced(raw)
}

fn bom_len(raw: &str) -> usize {
    usize::from(raw.starts_with('\u{feff}')) * '\u{feff}'.len_utf8()
}

fn fenced(raw: &str) -> Vec<Region> {
    let mut regions = Vec::new();
    let mut offset = bom_len(raw);
    let mut open: Option<Fence> = None;
    while offset < raw.len() {
        let next = raw[offset..].find('\n').map_or(raw.len(), |index| offset + index + 1);
        let line = raw[offset..next].trim_end_matches(['\r', '\n']);
        if let Some(fence) = &open {
            if closes(line, fence) {
                if fence.mermaid {
                    regions.push(Region {
                        index: regions.len(),
                        start: fence.content_start,
                        end: offset,
                    });
                }
                open = None;
            }
        } else if let Some(fence) = opens(line, next) {
            open = Some(fence);
        }
        offset = next;
    }
    regions
}

struct Fence {
    marker: char,
    length: usize,
    content_start: usize,
    mermaid: bool,
}

fn opens(line: &str, next: usize) -> Option<Fence> {
    let stripped = strip_blockquote(line);
    let rest = strip_indent(stripped, 3)?;
    let marker = rest.chars().next().filter(|character| matches!(character, '`' | '~'))?;
    let length = rest.chars().take_while(|character| *character == marker).count();
    if length < 3 {
        return None;
    }
    let info = rest[length..].trim();
    if marker == '`' && info.contains('`') {
        return None;
    }
    let language = info
        .split(|character: char| character.is_whitespace() || character == '{')
        .next()
        .unwrap_or("");
    Some(Fence {
        marker,
        length,
        content_start: next,
        mermaid: language.eq_ignore_ascii_case("mermaid"),
    })
}

fn closes(line: &str, fence: &Fence) -> bool {
    let Some(rest) = strip_indent(strip_blockquote(line), 3) else {
        return false;
    };
    let length = rest.chars().take_while(|character| *character == fence.marker).count();
    length >= fence.length && rest[length..].trim().is_empty()
}

fn strip_blockquote(line: &str) -> &str {
    let mut rest = line;
    loop {
        let trimmed = rest.trim_start_matches([' ', '\t']);
        let Some(after) = trimmed.strip_prefix('>') else {
            return rest;
        };
        rest = after.strip_prefix(' ').unwrap_or(after);
    }
}

fn strip_indent(line: &str, max: usize) -> Option<&str> {
    let mut taken = 0;
    let mut index = 0;
    for character in line.chars() {
        if character == ' ' && taken < max {
            taken += 1;
            index += 1;
        } else if character == '\t' && taken < max {
            taken = max;
            index += 1;
        } else {
            break;
        }
    }
    if taken > max {
        return None;
    }
    Some(&line[index..])
}
