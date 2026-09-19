use super::{BTreeSet, Bindings, Call, Token, TokenKind};
use crate::operations::syntax::matching_delimiter;

pub(super) fn assigned_variable(
    tokens: &[Token],
    source: &str,
    name_index: usize,
) -> Option<String> {
    let start = name_index.saturating_sub(12);
    let equals = (start..name_index)
        .rev()
        .find(|index| tokens[*index].text(source) == "=")?;
    (start..equals).rev().find_map(|index| {
        (tokens[index].kind == TokenKind::Identifier).then(|| tokens[index].text(source).to_owned())
    })
}

pub(super) fn call_name_index(tokens: &[Token], source: &str, open: usize) -> Option<usize> {
    let previous = open.checked_sub(1)?;
    if tokens[previous].kind == TokenKind::Identifier {
        return Some(previous);
    }
    if tokens[previous].text(source) == ">" {
        let mut depth = 0_usize;
        for index in (0..previous).rev() {
            match tokens[index].text(source) {
                ">" => depth += 1,
                "<" if depth == 0 => {
                    return index
                        .checked_sub(1)
                        .filter(|candidate| tokens[*candidate].kind == TokenKind::Identifier);
                }
                "<" => depth = depth.saturating_sub(1),
                _ => {}
            }
        }
    }
    (tokens[previous].text(source) == "!")
        .then(|| previous.checked_sub(1))
        .flatten()
        .filter(|index| tokens[*index].kind == TokenKind::Identifier)
}

pub(super) fn matching_close(tokens: &[Token], source: &str, open: usize) -> Option<usize> {
    matching_delimiter(tokens, source, open, ("(", ")"))
}

pub(super) fn call_chain(tokens: &[Token], source: &str, name_index: usize) -> String {
    let mut start = name_index;
    loop {
        if start >= 2
            && tokens[start - 1].text(source) == "."
            && tokens[start - 2].kind == TokenKind::Identifier
        {
            start -= 2;
        } else if start >= 3
            && tokens[start - 1].text(source) == ":"
            && tokens[start - 2].text(source) == ":"
            && tokens[start - 3].kind == TokenKind::Identifier
        {
            start -= 3;
        } else {
            break;
        }
    }
    tokens[start..=name_index]
        .iter()
        .map(|token| token.text(source))
        .collect::<String>()
}

pub(super) fn receiver_name(tokens: &[Token], source: &str, name_index: usize) -> Option<String> {
    if name_index >= 2
        && tokens[name_index - 1].text(source) == "."
        && tokens[name_index - 2].kind == TokenKind::Identifier
    {
        return Some(tokens[name_index - 2].text(source).to_owned());
    }
    (name_index >= 3
        && tokens[name_index - 1].text(source) == ":"
        && tokens[name_index - 2].text(source) == ":"
        && tokens[name_index - 3].kind == TokenKind::Identifier)
        .then(|| tokens[name_index - 3].text(source).to_owned())
}

pub(super) fn resource_values(
    call: &Call<'_, '_>,
    properties: &[&str],
    positional: usize,
    bindings: &Bindings,
) -> Vec<Option<String>> {
    let properties = property_values(call, properties);
    if properties.is_empty() {
        positional_values(call, positional, bindings)
    } else {
        properties.into_iter().map(Some).collect()
    }
}

pub(super) fn first_value(
    call: &Call<'_, '_>,
    properties: &[&str],
    positional: usize,
    bindings: &Bindings,
) -> Option<String> {
    resource_values(call, properties, positional, bindings)
        .into_iter()
        .flatten()
        .next()
}

pub(super) fn property(call: &Call<'_, '_>, names: &[&str]) -> Option<String> {
    property_values(call, names).into_iter().next()
}

pub(super) fn property_values(call: &Call<'_, '_>, names: &[&str]) -> Vec<String> {
    let names = names
        .iter()
        .map(|name| name.to_ascii_lowercase())
        .collect::<BTreeSet<_>>();
    let mut values = Vec::new();
    for index in 0..call.args.len() {
        if call.args[index].kind != TokenKind::Identifier
            || !names.contains(&call.args[index].text(call.source).to_ascii_lowercase())
        {
            continue;
        }
        let Some(separator) = call.args.get(index + 1) else {
            continue;
        };
        if !matches!(separator.text(call.source), ":" | "=" | "(") {
            continue;
        }
        let mut depth = 0_i32;
        for token in call.args.iter().skip(index + 2) {
            match token.text(call.source) {
                "[" | "{" | "(" => depth += 1,
                "]" | "}" | ")" => depth -= 1,
                "," if depth <= 0 => break,
                _ => {}
            }
            if token.kind == TokenKind::String
                && let Some(value) = literal_value(token.text(call.source))
            {
                values.push(value);
            }
        }
    }
    values
}

pub(super) fn positional_values(
    call: &Call<'_, '_>,
    position: usize,
    bindings: &Bindings,
) -> Vec<Option<String>> {
    let segments = argument_segments(call.args, call.source);
    let Some(segment) = segments.get(position) else {
        return vec![None];
    };
    let literals = segment
        .iter()
        .filter(|token| token.kind == TokenKind::String)
        .filter_map(|token| literal_value(token.text(call.source)))
        .map(Some)
        .collect::<Vec<_>>();
    if !literals.is_empty() {
        return literals;
    }
    let identifier = segment.iter().find_map(|token| {
        (token.kind == TokenKind::Identifier).then(|| token.text(call.source).to_owned())
    });
    if let Some(identifier) = identifier
        && let Some((_, value)) = bindings.resources.get(&identifier)
    {
        return vec![Some(value.clone())];
    }
    vec![None]
}

pub(super) fn positional_identifier(call: &Call<'_, '_>, position: usize) -> Option<String> {
    argument_segments(call.args, call.source)
        .get(position)?
        .iter()
        .find_map(|token| {
            (token.kind == TokenKind::Identifier).then(|| token.text(call.source).to_owned())
        })
}

pub(super) fn argument_segments<'tokens>(
    tokens: &'tokens [Token],
    source: &str,
) -> Vec<&'tokens [Token]> {
    let mut segments = Vec::new();
    let mut start = 0_usize;
    let mut depth = 0_i32;
    for (index, token) in tokens.iter().enumerate() {
        match token.text(source) {
            "(" | "[" | "{" => depth += 1,
            ")" | "]" | "}" => depth -= 1,
            "," if depth == 0 => {
                segments.push(&tokens[start..index]);
                start = index + 1;
            }
            _ => {}
        }
    }
    if start < tokens.len() {
        segments.push(&tokens[start..]);
    }
    segments
}

pub(super) fn literal_value(raw: &str) -> Option<String> {
    let trimmed = raw.trim();
    let quote = trimmed.find(['"', '\'', '`'])?;
    let mark = trimmed.as_bytes().get(quote).copied()? as char;
    let tail = &trimmed[quote + mark.len_utf8()..];
    let end = tail.rfind(mark)?;
    let value = &tail[..end];
    if value.is_empty() || value.contains("${") || value.contains("#{") {
        None
    } else {
        Some(value.to_owned())
    }
}

pub(super) fn source_line(source: &str, line: u32) -> String {
    source
        .lines()
        .nth(usize::try_from(line.saturating_sub(1)).unwrap_or(usize::MAX))
        .map(str::trim)
        .unwrap_or_default()
        .chars()
        .take(300)
        .collect()
}

pub(super) fn non_empty_resources(resources: Vec<Option<String>>) -> Vec<Option<String>> {
    if resources.is_empty() {
        vec![None]
    } else {
        resources
    }
}
