use super::code_span;
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
    let code = code_span::mask(raw);
    if !has_library_import(imports, raw, &code) {
        return Vec::new();
    }
    let mut occurrences = Vec::new();
    let mut search_from = 0_usize;
    while let Some(rel) = next_api(raw, search_from, &code) {
        let start = rel.0;
        let api = rel.1;
        let body = object_after(raw, start + api.len(), &code);
        let Some((object, open, end)) = body else {
            search_from = start + api.len();
            continue;
        };
        if unknown_spread_overwrites(object, open, &code) {
            occurrences.push(ConsumerOccurrence {
                api: api.to_owned(),
                member_name: literal_prop(object, "functionName", open, &code)
                    .or_else(|| literal_prop(object, "eventName", open, &code)),
                member_kind: member_kind(api),
                abi_local: ident_prop(object, "abi", open, &code),
                import_path: None,
                resolved: false,
                span: span_for(path, raw, start, end),
                ordinal: occurrences.len() + 1,
            });
            search_from = end;
            continue;
        }
        let abi_local = ident_prop(object, "abi", open, &code);
        let import_path = abi_local
            .as_deref()
            .and_then(|local| import_target(imports, local))
            .and_then(|spec| resolve_import(path, spec));
        let resolved = import_path.is_some();
        occurrences.push(ConsumerOccurrence {
            api: api.to_owned(),
            member_name: literal_prop(object, "functionName", open, &code)
                .or_else(|| literal_prop(object, "eventName", open, &code)),
            member_kind: member_kind(api),
            abi_local,
            import_path,
            resolved,
            span: span_for(path, raw, start, end),
            ordinal: occurrences.len() + 1,
        });
        search_from = end;
    }
    occurrences
}

fn has_library_import(imports: &[ImportFact], raw: &str, code: &[bool]) -> bool {
    imports.iter().any(is_web3_import) || named_import(raw, code)
}

fn is_web3_import(import: &ImportFact) -> bool {
    let target = import.target.as_str();
    target == "viem"
        || target == "wagmi"
        || target.starts_with("viem/")
        || target.starts_with("wagmi/")
        || target.starts_with("@wagmi/")
}

fn named_import(raw: &str, code: &[bool]) -> bool {
    raw.match_indices("from ")
        .filter(|(index, _)| code_span::at(code, *index))
        .any(|(index, _)| {
            let rest = raw[index + 5..].trim_start();
            rest.starts_with("\"viem\"")
                || rest.starts_with("'viem'")
                || rest.starts_with("\"wagmi\"")
                || rest.starts_with("'wagmi'")
                || rest.starts_with("\"viem/")
                || rest.starts_with("'viem/")
                || rest.starts_with("\"wagmi/")
                || rest.starts_with("'wagmi/")
                || rest.starts_with("\"@wagmi/")
                || rest.starts_with("'@wagmi/")
        })
}

fn next_api(raw: &str, from: usize, code: &[bool]) -> Option<(usize, &'static str)> {
    let rest = raw.get(from..)?;
    let mut best: Option<(usize, &'static str)> = None;
    for api in VIEM_APIS {
        let mut offset = 0;
        while let Some(at) = rest.get(offset..).and_then(|slice| slice.find(api)) {
            let local = offset + at;
            let absolute = from + local;
            if code_span::at(code, absolute)
                && code_span::ident_start(raw, absolute)
                && is_call(&rest[local + api.len()..])
            {
                if best.is_none_or(|(current, _)| local < current) {
                    best = Some((absolute, api));
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

fn object_after<'a>(raw: &'a str, from: usize, code: &[bool]) -> Option<(&'a str, usize, usize)> {
    let rel = raw.get(from..)?.char_indices().find_map(|(index, ch)| {
        (ch == '{' && code_span::at(code, from + index)).then_some(index)
    })?;
    let open = from + rel;
    let mut depth = 0_i32;
    for (index, ch) in raw[open..].char_indices() {
        if !code_span::at(code, open + index) {
            continue;
        }
        match ch {
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    let end = open + index + 1;
                    return Some((&raw[open..end], open, end));
                }
            }
            _ => {}
        }
    }
    None
}

fn unknown_spread_overwrites(object: &str, open: usize, code: &[bool]) -> bool {
    object
        .match_indices("...")
        .any(|(at, _)| code_span::at(code, open + at))
}

fn literal_prop(object: &str, key: &str, open: usize, code: &[bool]) -> Option<String> {
    let patterns = [
        format!("{key}:"),
        format!("{key} :"),
        format!("\"{key}\":"),
        format!("'{key}':"),
    ];
    for pattern in patterns {
        let mut from = 0;
        while let Some(rel) = object[from..].find(&pattern) {
            let at = from + rel;
            if code_span::at(code, open + at) {
                let rest = object[at + pattern.len()..].trim_start();
                if let Some(value) = quoted(rest) {
                    return Some(value);
                }
            }
            from = at + pattern.len();
        }
    }
    None
}

fn ident_prop(object: &str, key: &str, open: usize, code: &[bool]) -> Option<String> {
    let pattern = format!("{key}:");
    let mut from = 0;
    while let Some(rel) = object[from..].find(&pattern) {
        let at = from + rel;
        if code_span::at(code, open + at) {
            let rest = object[at + pattern.len()..].trim_start();
            let ident = rest
                .chars()
                .take_while(|ch| ch.is_ascii_alphanumeric() || *ch == '_')
                .collect::<String>();
            if !ident.is_empty() {
                return Some(ident);
            }
        }
        from = at + pattern.len();
    }
    None
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
}

fn member_kind(api: &str) -> &'static str {
    if api.contains("Event") || api.contains("Log") {
        "event"
    } else {
        "function"
    }
}
