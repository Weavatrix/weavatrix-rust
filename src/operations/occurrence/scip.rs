//! Read an on-disk SCIP index. Never spawn `scip-*`.
//!
//! github/stack-graphs was archived on 2025-09-09; this crate does not take
//! it as a live dependency and does not vendor it.

use super::path::{normalize, repo_relative_file};
use super::position::Query;
use crate::engine::RepositoryState;
use std::fs;
use weavatrix_graph::{SourcePosition, SourceSpan};

const MAX_BYTES: usize = 32 * 1024 * 1024;
const DISCOVERY: &[&str] = &["index.scip", ".scip/index.scip"];
const DEFINITION: i32 = 0x1;

#[derive(Debug, Clone)]
pub(crate) struct Loaded {
    pub relative: String,
    occurrences: Vec<Occurrence>,
}

#[derive(Debug, Clone)]
struct Occurrence {
    symbol: String,
    roles: i32,
    span: SourceSpan,
}

#[derive(Debug, Clone)]
pub(crate) struct ScipHit {
    pub symbol: String,
    pub usage: SourceSpan,
    pub definition: Option<SourceSpan>,
}

/// Loads the caller-named SCIP file, or the first well-known name that
/// already exists. Absence is not an error; a named missing file is.
pub(super) fn load(
    state: &RepositoryState,
    explicit: Option<&str>,
) -> Result<Option<Loaded>, String> {
    let relative = match explicit {
        Some(path) => normalize(path),
        None => match DISCOVERY
            .iter()
            .copied()
            .find(|candidate| state.root().join(candidate).is_file())
        {
            Some(path) => path.to_owned(),
            None => return Ok(None),
        },
    };
    let path = repo_relative_file(state.root(), &relative)?;
    let bytes = fs::read(&path).map_err(|error| format!("cannot read {relative}: {error}"))?;
    if bytes.len() > MAX_BYTES {
        return Err(format!(
            "{relative} is {} bytes; SCIP indexes above {MAX_BYTES} bytes are refused",
            bytes.len()
        ));
    }
    Ok(Some(Loaded {
        relative,
        occurrences: parse_index(&bytes)?,
    }))
}

impl Loaded {
    pub(super) fn at_position(&self, query: &Query) -> Option<ScipHit> {
        let usage = self
            .occurrences
            .iter()
            .find(|item| contains(&item.span, query))?;
        let definition = self
            .occurrences
            .iter()
            .find(|item| item.symbol == usage.symbol && item.roles & DEFINITION != 0)
            .map(|item| item.span.clone());
        Some(ScipHit {
            symbol: usage.symbol.clone(),
            usage: usage.span.clone(),
            definition,
        })
    }

    pub(super) fn references(&self, symbol: &str) -> Vec<SourceSpan> {
        self.occurrences
            .iter()
            .filter(|item| item.symbol == symbol)
            .map(|item| item.span.clone())
            .collect()
    }

    pub(super) fn symbol_at(&self, query: &Query) -> Option<&str> {
        self.occurrences
            .iter()
            .find(|item| contains(&item.span, query))
            .map(|item| item.symbol.as_str())
    }
}

fn contains(span: &SourceSpan, query: &Query) -> bool {
    if normalize(span.file.as_str()) != query.path {
        return false;
    }
    let start = (span.start.line, span.start.column);
    let end = (span.end.line, span.end.column);
    let at = (query.line, query.column);
    if start == end {
        return at == start;
    }
    start <= at && at < end
}

fn parse_index(bytes: &[u8]) -> Result<Vec<Occurrence>, String> {
    let mut items = Vec::new();
    let mut cursor = 0;
    while cursor < bytes.len() {
        let (field, wire, payload) = read_field(bytes, &mut cursor)?;
        if field == 2 && wire == 2 {
            parse_document(payload, &mut items)?;
        }
    }
    Ok(items)
}

fn parse_document(bytes: &[u8], items: &mut Vec<Occurrence>) -> Result<(), String> {
    let mut path = String::new();
    let mut cursor = 0;
    let mut pending = Vec::new();
    while cursor < bytes.len() {
        let (field, wire, payload) = read_field(bytes, &mut cursor)?;
        match (field, wire) {
            (2, 2) => path = String::from_utf8_lossy(payload).into_owned(),
            (3, 2) => pending.push(payload.to_vec()),
            _ => {}
        }
    }
    if path.is_empty() {
        return Ok(());
    }
    let path = normalize(&path);
    for message in pending {
        if let Some(item) = parse_occurrence(&message, &path)? {
            items.push(item);
        }
    }
    Ok(())
}

fn parse_occurrence(bytes: &[u8], path: &str) -> Result<Option<Occurrence>, String> {
    let mut range = Vec::new();
    let mut symbol = String::new();
    let mut roles = 0_i32;
    let mut cursor = 0;
    while cursor < bytes.len() {
        let (field, wire, payload) = read_field(bytes, &mut cursor)?;
        match (field, wire) {
            (1, 2) => decode_packed_i32(payload, &mut range)?,
            (1, 0) => range.push(decode_i32(payload)?),
            (2, 2) => symbol = String::from_utf8_lossy(payload).into_owned(),
            (3, 0) => roles = decode_i32(payload)?,
            _ => {}
        }
    }
    if symbol.is_empty() {
        return Ok(None);
    }
    let Some(span) = scip_span(path, &range) else {
        return Ok(None);
    };
    Ok(Some(Occurrence {
        symbol,
        roles,
        span,
    }))
}

fn scip_span(path: &str, range: &[i32]) -> Option<SourceSpan> {
    let (start_line, start_col, end_line, end_col) = match *range {
        [line, start, end] => (line, start, line, end),
        [start_line, start_col, end_line, end_col] => (start_line, start_col, end_line, end_col),
        _ => return None,
    };
    Some(SourceSpan::new(
        path,
        SourcePosition::new(one_based(start_line)?, one_based(start_col)?),
        SourcePosition::new(one_based(end_line)?, one_based(end_col)?),
    ))
}

fn one_based(value: i32) -> Option<u32> {
    u32::try_from(value.checked_add(1)?).ok()
}

fn read_field<'a>(bytes: &'a [u8], cursor: &mut usize) -> Result<(u32, u32, &'a [u8]), String> {
    let key = read_varint(bytes, cursor)?;
    let field = u32::try_from(key >> 3).map_err(|_| "SCIP field number is invalid".to_owned())?;
    let wire = u32::try_from(key & 7).map_err(|_| "SCIP wire type is invalid".to_owned())?;
    let payload = match wire {
        0 => {
            let start = *cursor;
            let _ = read_varint(bytes, cursor)?;
            &bytes[start..*cursor]
        }
        2 => {
            let length = usize::try_from(read_varint(bytes, cursor)?)
                .map_err(|_| "SCIP length is invalid".to_owned())?;
            if bytes.len().saturating_sub(*cursor) < length {
                return Err("SCIP message is truncated".to_owned());
            }
            let start = *cursor;
            *cursor += length;
            &bytes[start..*cursor]
        }
        1 => take(bytes, cursor, 8)?,
        5 => take(bytes, cursor, 4)?,
        _ => return Err("SCIP wire type is unsupported".to_owned()),
    };
    Ok((field, wire, payload))
}

fn take<'a>(bytes: &'a [u8], cursor: &mut usize, count: usize) -> Result<&'a [u8], String> {
    if bytes.len().saturating_sub(*cursor) < count {
        return Err("SCIP message is truncated".to_owned());
    }
    let start = *cursor;
    *cursor += count;
    Ok(&bytes[start..*cursor])
}

fn decode_packed_i32(bytes: &[u8], output: &mut Vec<i32>) -> Result<(), String> {
    let mut cursor = 0;
    while cursor < bytes.len() {
        let raw = read_varint(bytes, &mut cursor)?;
        output.push(decode_i32_value(raw)?);
    }
    Ok(())
}

fn decode_i32(bytes: &[u8]) -> Result<i32, String> {
    let mut cursor = 0;
    decode_i32_value(read_varint(bytes, &mut cursor)?)
}

fn decode_i32_value(value: u64) -> Result<i32, String> {
    i32::try_from(value).map_err(|_| "SCIP integer is out of range".to_owned())
}

fn read_varint(bytes: &[u8], cursor: &mut usize) -> Result<u64, String> {
    let mut value = 0_u64;
    let mut shift = 0;
    loop {
        let byte = *bytes
            .get(*cursor)
            .ok_or_else(|| "SCIP varint is truncated".to_owned())?;
        *cursor += 1;
        value |= u64::from(byte & 0x7f) << shift;
        if byte & 0x80 == 0 {
            return Ok(value);
        }
        shift += 7;
        if shift > 63 {
            return Err("SCIP varint is too long".to_owned());
        }
    }
}
