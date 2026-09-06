use super::{
    Call, Language, Path, RepositoryState, Token, TokenKind, argument_segments, call_chain,
    call_name_index, literal_value, matching_close, property, source_line, tokenize,
};
use std::collections::HashMap;
use std::fs;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum HttpMethodEvidence {
    Proven(&'static str),
    Unresolved,
}

impl HttpMethodEvidence {
    pub(super) fn as_json(self) -> Option<&'static str> {
        match self {
            Self::Proven(method) => Some(method),
            Self::Unresolved => Some("UNRESOLVED"),
        }
    }

    pub(super) fn mismatches(self, backend_method: &str) -> bool {
        match self {
            Self::Unresolved => false,
            Self::Proven(method) => {
                backend_method != "ANY" && backend_method != "ALL" && method != backend_method
            }
        }
    }
}

pub(super) struct ProvenCallsite {
    pub(super) line: u32,
    pub(super) method: HttpMethodEvidence,
    pub(super) exact_literal: bool,
    pub(super) evidence: String,
}

pub(super) struct SourceCache {
    files: HashMap<String, Option<(Language, String)>>,
}

impl SourceCache {
    pub(super) fn new() -> Self {
        Self {
            files: HashMap::new(),
        }
    }

    pub(super) fn resolve_callsite(
        &mut self,
        client: &RepositoryState,
        path: &str,
        line: u32,
        route: &str,
    ) -> Option<ProvenCallsite> {
        let (language, source) = self.load(client, path)?;
        find_proven_callsite(source, language, route, line)
    }

    fn load(&mut self, client: &RepositoryState, path: &str) -> Option<(Language, &str)> {
        if !self.files.contains_key(path) {
            let loaded = super::scan::language_for_path(path).and_then(|language| {
                let relative = Path::new(path);
                if relative.is_absolute()
                    || relative.components().any(|component| {
                        matches!(
                            component,
                            std::path::Component::ParentDir
                                | std::path::Component::RootDir
                                | std::path::Component::Prefix(_)
                        )
                    })
                {
                    return None;
                }
                fs::read_to_string(client.root().join(relative))
                    .ok()
                    .map(|source| (language, source))
            });
            self.files.insert(path.to_owned(), loaded);
        }
        self.files
            .get(path)
            .and_then(|entry| entry.as_ref())
            .map(|(language, source)| (*language, source.as_str()))
    }
}

pub(super) fn find_proven_callsite(
    source: &str,
    language: Language,
    route: &str,
    line: u32,
) -> Option<ProvenCallsite> {
    let tokens = tokenize(source, language)
        .into_iter()
        .filter(|token| !token.is_trivia())
        .collect::<Vec<_>>();
    let mut index = 0_usize;
    while index < tokens.len() {
        if tokens[index].text(source) != "(" {
            index += 1;
            continue;
        }
        let Some(name_index) = call_name_index(&tokens, source, index) else {
            index += 1;
            continue;
        };
        let Some(close) = matching_close(&tokens, source, index) else {
            break;
        };
        let name = tokens[name_index].text(source).to_owned();
        let chain = call_chain(&tokens, source, name_index);
        let call = Call {
            name,
            chain,
            receiver: None,
            args: &tokens[index + 1..close],
            line: tokens[name_index].line,
            column: tokens[name_index].column,
            evidence: source_line(source, tokens[name_index].line),
            source,
        };
        if call_has_route_literal_on_line(&call, route, line) && looks_like_http_call(&call) {
            let exact_literal = call.args.iter().any(|token| {
                token.kind == TokenKind::String
                    && token.line == line
                    && literal_value(token.text(source)).is_some_and(|value| value.contains(route))
            });
            return Some(ProvenCallsite {
                line: call.line,
                method: resolve_method(&call),
                exact_literal,
                evidence: call.evidence,
            });
        }
        index += 1;
    }
    None
}

fn call_has_route_literal_on_line(call: &Call<'_, '_>, route: &str, line: u32) -> bool {
    call.args.iter().any(|token| {
        token.kind == TokenKind::String
            && token.line == line
            && literal_value(token.text(call.source))
                .is_some_and(|value| super::http_route::route_matches(route, &value))
    })
}

fn looks_like_http_call(call: &Call<'_, '_>) -> bool {
    let name = call.name.to_ascii_lowercase();
    let chain = call.chain.to_ascii_lowercase();
    if matches!(
        name.as_str(),
        "fetch" | "request" | "ajax" | "axios" | "got" | "ofetch" | "ky"
    ) {
        return true;
    }
    if chain.contains("axios") || chain.contains("httpclient") || chain.contains("xmlhttprequest") {
        return true;
    }
    ["delete", "patch", "post", "put", "head", "options", "get"]
        .into_iter()
        .any(|verb| chain == verb || chain.ends_with(&format!(".{verb}")))
}

pub(super) fn resolve_method(call: &Call<'_, '_>) -> HttpMethodEvidence {
    let chain = call.chain.to_ascii_lowercase();
    for (suffix, method) in [
        (".delete", "DELETE"),
        (".patch", "PATCH"),
        (".post", "POST"),
        (".put", "PUT"),
        (".head", "HEAD"),
        (".options", "OPTIONS"),
        (".get", "GET"),
    ] {
        if chain == suffix.trim_start_matches('.') || chain.ends_with(suffix) {
            return HttpMethodEvidence::Proven(method);
        }
    }
    if let Some(value) = property(call, &["method", "httpmethod"]) {
        return normalize_method(&value)
            .map(HttpMethodEvidence::Proven)
            .unwrap_or(HttpMethodEvidence::Unresolved);
    }
    if has_method_key(call) {
        return HttpMethodEvidence::Unresolved;
    }
    let segments = argument_segments(call.args, call.source);
    if segments
        .get(1)
        .is_some_and(|segment| is_sole_identifier(segment))
    {
        return HttpMethodEvidence::Unresolved;
    }
    if call.name.eq_ignore_ascii_case("fetch")
        || call.name.eq_ignore_ascii_case("axios")
        || call.name.eq_ignore_ascii_case("ofetch")
        || call.name.eq_ignore_ascii_case("ky")
        || call.name.eq_ignore_ascii_case("got")
    {
        return HttpMethodEvidence::Proven("GET");
    }
    HttpMethodEvidence::Unresolved
}

fn normalize_method(value: &str) -> Option<&'static str> {
    match value.trim().to_ascii_uppercase().as_str() {
        "DELETE" => Some("DELETE"),
        "PATCH" => Some("PATCH"),
        "POST" => Some("POST"),
        "PUT" => Some("PUT"),
        "HEAD" => Some("HEAD"),
        "OPTIONS" => Some("OPTIONS"),
        "GET" => Some("GET"),
        _ => None,
    }
}

fn has_method_key(call: &Call<'_, '_>) -> bool {
    call.args.iter().enumerate().any(|(index, token)| {
        if token.kind != TokenKind::Identifier {
            return false;
        }
        let key = token.text(call.source).to_ascii_lowercase();
        if key != "method" && key != "httpmethod" {
            return false;
        }
        call.args
            .get(index + 1)
            .is_some_and(|separator| matches!(separator.text(call.source), ":" | "="))
    })
}

fn is_sole_identifier(segment: &[Token]) -> bool {
    matches!(segment, [token] if token.kind == TokenKind::Identifier)
}
