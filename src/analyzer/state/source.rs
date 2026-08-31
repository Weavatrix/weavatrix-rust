use crate::language::{FileFacts, Language, LanguageRegistry};
use crate::model::Result;
use std::path::Path;

/// A file parsed off the graph thread; integration stays sequential and
/// deterministic while parsing fans out across cores.
pub(in crate::analyzer) struct ParsedSource {
    pub(in crate::analyzer) relative: String,
    pub(in crate::analyzer) bytes: u64,
    pub(in crate::analyzer) content_hash: Option<String>,
    pub(in crate::analyzer) transport_candidate: bool,
    pub(in crate::analyzer) outcome: ParseOutcome,
}

pub(in crate::analyzer) enum ParseOutcome {
    Skipped,
    NonUtf8,
    /// Boxed so the enum stays small: parsed facts dominate its size and most
    /// scanned entries carry no facts at all.
    Parsed {
        language: Language,
        extractor: &'static str,
        facts: Box<FileFacts>,
    },
}

/// Parses one source blob into integration-ready facts. Pure with respect to
/// analysis state, so it is safe to run from worker threads.
pub(in crate::analyzer) fn parse_source(
    relative: &str,
    bytes: &[u8],
    content_hash: Option<&str>,
    registry: &LanguageRegistry,
) -> Result<ParsedSource> {
    let size = u64::try_from(bytes.len()).unwrap_or(u64::MAX);
    let sourced = |outcome: ParseOutcome| ParsedSource {
        relative: relative.to_owned(),
        bytes: size,
        content_hash: content_hash.map(str::to_owned),
        transport_candidate: false,
        outcome,
    };
    let Ok(text) = std::str::from_utf8(bytes) else {
        return Ok(sourced(ParseOutcome::NonUtf8));
    };
    let extension = Path::new(relative)
        .extension()
        .and_then(|value| value.to_str())
        .map(str::to_ascii_lowercase)
        .unwrap_or_default();
    let Some(adapter) = registry.adapter_for_extension(&extension) else {
        return Ok(sourced(ParseOutcome::Skipped));
    };
    let mut facts = adapter.parse(crate::language::SourceFile {
        path: relative,
        text,
    })?;
    for symbol in &mut facts.symbols {
        let source_extent = symbol.source_extent.as_ref().unwrap_or(&symbol.span);
        symbol.source_fingerprint = declaration_text(text, source_extent).map(stable_fingerprint);
    }
    // Property reads carry the same member operator as member calls, so the
    // repair applies to every reference kind, not only `Calls`.
    for reference in &mut facts.references {
        if !reference.qualified && qualified_at_span(text, &reference.span) {
            reference.qualified = true;
        }
    }
    let transport_candidate = crate::language::file_facts_have_transport_evidence(&facts);
    let mut parsed = sourced(ParseOutcome::Parsed {
        language: adapter.language(),
        extractor: adapter.extractor(),
        facts: Box::new(facts),
    });
    parsed.transport_candidate = transport_candidate;
    Ok(parsed)
}

fn declaration_text<'source>(
    source: &'source str,
    span: &crate::SourceSpan,
) -> Option<&'source str> {
    let start = byte_offset(source, span.start)?;
    let end = byte_offset(source, span.end)?;
    (start <= end).then(|| &source[start..end])
}

fn byte_offset(source: &str, position: crate::SourcePosition) -> Option<usize> {
    let line_index = usize::try_from(position.line.checked_sub(1)?).ok()?;
    let column = usize::try_from(position.column.checked_sub(1)?).ok()?;
    let line_start = source
        .split_inclusive('\n')
        .take(line_index)
        .map(str::len)
        .sum::<usize>();
    let line = source
        .get(line_start..)?
        .split_once('\n')
        .map_or_else(|| source.get(line_start..), |(line, _)| Some(line))?;
    (column <= line.len() && line.is_char_boundary(column)).then_some(line_start + column)
}

fn stable_fingerprint(source: &str) -> String {
    let mut hash = 0xcbf2_9ce4_8422_2325_u64;
    for byte in source.bytes() {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    format!("fnv1a64:{hash:016x}")
}

/// Expression receivers do not have a concrete name, but their member/path
/// operator is exact source evidence and must survive into call resolution.
fn qualified_at_span(text: &str, span: &crate::SourceSpan) -> bool {
    let Some(line) = text.lines().nth(span.start.line.saturating_sub(1) as usize) else {
        return false;
    };
    let column = span.start.column.saturating_sub(1) as usize;
    // Lossless-parser columns count source characters, not UTF-8 bytes.
    let prefix = line.chars().take(column).collect::<String>();
    let prefix = prefix.trim_end();
    (prefix.ends_with('.') && !prefix.ends_with("..")) || prefix.ends_with("::")
}
