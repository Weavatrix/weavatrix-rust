use crate::language::FileFacts;
use weavatrix_graph::{SourcePosition, SourceSpan};

pub(super) fn line_span(path: &str, line: u32, raw: &str) -> SourceSpan {
    SourceSpan {
        file: path.to_owned(),
        start: SourcePosition { line, column: 1 },
        end: SourcePosition {
            line,
            column: u32::try_from(raw.len() + 1).unwrap_or(u32::MAX),
        },
    }
}

pub(super) fn line_number(offset: usize) -> u32 {
    u32::try_from(offset).unwrap_or(u32::MAX).saturating_add(1)
}

pub(super) fn identifier(value: &str) -> &str {
    value
        .trim_start()
        .split(|character: char| !is_ident(character))
        .next()
        .unwrap_or_default()
}

pub(super) fn sql_name(value: &str) -> &str {
    value
        .trim_start()
        .trim_start_matches("IF NOT EXISTS ")
        .split(|character: char| {
            character.is_ascii_whitespace() || matches!(character, '(' | ';' | ',')
        })
        .next()
        .unwrap_or_default()
        .trim_matches(|character| matches!(character, '"' | '`' | '[' | ']'))
}

pub(super) fn word_suffix<'line>(line: &'line str, prefix: &str) -> Option<&'line str> {
    let index = line.find(prefix)?;
    let valid = index == 0
        || line[..index]
            .chars()
            .last()
            .is_some_and(char::is_whitespace);
    valid.then(|| &line[index + prefix.len()..])
}

pub(super) fn is_ident(character: char) -> bool {
    character.is_ascii_alphanumeric() || matches!(character, '_' | '$')
}

pub(super) fn control_word(name: &str) -> bool {
    matches!(
        name,
        "if" | "for" | "while" | "switch" | "match" | "catch" | "return" | "sizeof" | "new"
    )
}

pub(super) fn brace_delta(line: &str) -> i32 {
    let opens = i32::try_from(line.matches('{').count()).unwrap_or(i32::MAX);
    let closes = i32::try_from(line.matches('}').count()).unwrap_or(i32::MAX);
    opens.saturating_sub(closes)
}

pub(super) fn sort_facts(facts: &mut FileFacts) {
    facts
        .symbols
        .sort_by(|left, right| left.span.cmp(&right.span));
    facts
        .references
        .sort_by(|left, right| left.span.cmp(&right.span));
    facts
        .imports
        .sort_by(|left, right| left.span.cmp(&right.span));
    facts.domains.sort_by(|left, right| {
        (&left.kind, &left.name, &left.span).cmp(&(&right.kind, &right.name, &right.span))
    });
}
