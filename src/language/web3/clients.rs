use super::identity::resolve_import;
use super::span::span_for;
use crate::language::ImportFact;
use weavatrix_graph::SourceSpan;

const VIEM_APIS: &[&str] = &[
    "readContract",
    "writeContract",
    "simulateContract",
    "getContract",
    "watchContractEvent",
    "getContractEvents",
    "encodeFunctionData",
    "decodeFunctionResult",
    "decodeEventLog",
    "parseEventLogs",
];

#[derive(Debug, Clone)]
pub(super) struct ConsumerOccurrence {
    pub api: String,
    pub member_name: Option<String>,
    pub member_kind: &'static str,
    pub abi_local: Option<String>,
    pub import_path: Option<String>,
    pub resolved: bool,
    pub span: SourceSpan,
    pub ordinal: usize,
}

#[must_use]
pub(super) fn extract(path: &str, raw: &str, imports: &[ImportFact]) -> Vec<ConsumerOccurrence> {
    if !has_library_import(imports, raw) {
        return Vec::new();
    }
    let mut occurrences = Vec::new();
    let mut search_from = 0_usize;
    while let Some(rel) = next_api(raw, search_from) {
        let start = rel.0;
        let api = rel.1;
        let body = object_after(raw, start + api.len());
        let Some((object, end)) = body else {
            search_from = start + api.len();
            continue;
        };
        if unknown_spread_overwrites(object) {
            occurrences.push(ConsumerOccurrence {
                api: api.to_owned(),
                member_name: literal_prop(object, "functionName")
                    .or_else(|| literal_prop(object, "eventName")),
                member_kind: member_kind(api),
                abi_local: ident_prop(object, "abi"),
                import_path: None,
                resolved: false,
                span: span_for(path, raw, start, end),
                ordinal: occurrences.len() + 1,
            });
            search_from = end;
            continue;
        }
        let abi_local = ident_prop(object, "abi");
        let import_path = abi_local
            .as_deref()
            .and_then(|local| import_target(imports, local))
            .or_else(|| json_spec(raw))
            .and_then(|spec| resolve_import(path, spec));
        occurrences.push(ConsumerOccurrence {
            api: api.to_owned(),
            member_name: literal_prop(object, "functionName")
                .or_else(|| literal_prop(object, "eventName")),
            member_kind: member_kind(api),
            abi_local,
            import_path,
            resolved: true,
            span: span_for(path, raw, start, end),
            ordinal: occurrences.len() + 1,
        });
        search_from = end;
    }
    occurrences
}

fn has_library_import(imports: &[ImportFact], raw: &str) -> bool {
    imports.iter().any(|import| {
        let target = import.target.as_str();
        target == "viem"
            || target == "wagmi"
            || target.starts_with("viem/")
            || target.starts_with("wagmi/")
            || target.starts_with("@wagmi/")
    }) || raw.contains("from \"viem\"")
        || raw.contains("from 'viem'")
        || raw.contains("from \"wagmi\"")
        || raw.contains("from 'wagmi'")
}

fn json_spec(raw: &str) -> Option<&str> {
    for quote in ['"', '\''] {
        let marker = format!("from {quote}");
        if let Some(at) = raw.find(&marker) {
            let rest = &raw[at + marker.len()..];
            if let Some(end) = rest.find(quote) {
                let spec = &rest[..end];
                if std::path::Path::new(spec)
                    .extension()
                    .is_some_and(|ext| ext.eq_ignore_ascii_case("json"))
                {
                    return Some(spec);
                }
            }
        }
    }
    None
}

fn next_api(raw: &str, from: usize) -> Option<(usize, &'static str)> {
    let rest = raw.get(from..)?;
    let mut best: Option<(usize, &'static str)> = None;
    for api in VIEM_APIS {
        let mut offset = 0;
        while let Some(at) = rest.get(offset..).and_then(|slice| slice.find(api)) {
            let local = offset + at;
            if is_call(&rest[local + api.len()..]) {
                if best.is_none_or(|(current, _)| local < current) {
                    best = Some((from + local, api));
                }
                break;
            }
            offset = local + api.len();
        }
    }
    best
}

fn is_call(after: &str) -> bool {
    after.trim_start().starts_with('(')
}

fn object_after(raw: &str, from: usize) -> Option<(&str, usize)> {
    let open = raw.get(from..)?.find('{')? + from;
    let mut depth = 0_i32;
    for (index, ch) in raw[open..].char_indices() {
        match ch {
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    let end = open + index + 1;
                    return Some((&raw[open..end], end));
                }
            }
            _ => {}
        }
    }
    None
}

fn unknown_spread_overwrites(object: &str) -> bool {
    object.contains("...")
}

fn literal_prop(object: &str, key: &str) -> Option<String> {
    let patterns = [
        format!("{key}:"),
        format!("{key} :"),
        format!("\"{key}\":"),
        format!("'{key}':"),
    ];
    for pattern in patterns {
        if let Some(at) = object.find(&pattern) {
            let rest = object[at + pattern.len()..].trim_start();
            if let Some(value) = quoted(rest) {
                return Some(value);
            }
        }
    }
    None
}

fn ident_prop(object: &str, key: &str) -> Option<String> {
    let rest = object.split(&format!("{key}:")).nth(1)?.trim_start();
    let ident = rest
        .chars()
        .take_while(|ch| ch.is_ascii_alphanumeric() || *ch == '_')
        .collect::<String>();
    if ident.is_empty() { None } else { Some(ident) }
}

fn quoted(rest: &str) -> Option<String> {
    let mut chars = rest.chars();
    let quote = chars.next()?;
    if quote != '\'' && quote != '"' {
        return None;
    }
    let mut out = String::new();
    for ch in chars {
        if ch == quote {
            return Some(out);
        }
        out.push(ch);
    }
    None
}

fn import_target<'a>(imports: &'a [ImportFact], local: &str) -> Option<&'a str> {
    imports
        .iter()
        .find(|import| import.bindings.iter().any(|binding| binding.local == local))
        .map(|import| import.target.as_str())
        .or_else(|| {
            imports
                .iter()
                .find(|import| {
                    std::path::Path::new(&import.target)
                        .extension()
                        .is_some_and(|ext| ext.eq_ignore_ascii_case("json"))
                })
                .map(|import| import.target.as_str())
        })
}

fn member_kind(api: &str) -> &'static str {
    if api.contains("Event") || api.contains("Log") {
        "event"
    } else {
        "function"
    }
}
