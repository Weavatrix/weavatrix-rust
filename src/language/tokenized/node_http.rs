//! Hand-rolled script HTTP evidence that never goes through a router call:
//! `if (req.method === "GET" && url.pathname === "/ping")` conditions become
//! endpoints, and `BrowserWindow.loadFile("renderer/index.html")` becomes an
//! import of the page it opens.

use crate::language::{DomainFact, FileFacts, ImportFact};
use weavatrix_graph::{EdgeKind, NodeKind, SourcePosition, SourceSpan};
use weavatrix_parse::{Token, TokenKind};

pub(super) fn is_script_source(path: &str) -> bool {
    std::path::Path::new(path)
        .extension()
        .and_then(|value| value.to_str())
        .is_some_and(|extension| {
            ["js", "jsx", "mjs", "cjs", "ts", "tsx", "mts", "cts"]
                .iter()
                .any(|known| extension.eq_ignore_ascii_case(known))
        })
}

const HTTP_VERBS: &[&str] = &["GET", "POST", "PUT", "DELETE", "PATCH", "HEAD", "OPTIONS"];
const PATH_PROPERTIES: &[&str] = &["pathname", "path", "url"];

pub(super) fn apply(
    path: &str,
    text: &str,
    parse: weavatrix_parse::Language,
    converted: &mut FileFacts,
) {
    let tokens = weavatrix_parse::tokenize_lite(text, parse);
    let serves = tokens
        .iter()
        .any(|token| token.kind == TokenKind::Identifier && token.text(text) == "createServer");
    let mut index = 0_usize;
    while index < tokens.len() {
        let word = tokens[index].text(text);
        if word == "if"
            && let Some(open) = opens_at(&tokens, text, index + 1)
            && let Some(close) = matching_paren(&tokens, text, open)
        {
            condition_routes(path, text, &tokens[open + 1..close], serves, converted);
        }
        if (word == "loadFile" || word == "loadURL")
            && let Some(open) = opens_at(&tokens, text, index + 1)
            && let Some(close) = matching_paren(&tokens, text, open)
        {
            page_imports(path, text, &tokens[open + 1..close], converted);
            index = close + 1;
            continue;
        }
        index += 1;
    }
}

/// Endpoints proven by one condition: every method literal times every route
/// literal compared inside it. A path-only condition counts only in a file
/// that builds its own server, so client-side routers stay out.
fn condition_routes(
    path: &str,
    text: &str,
    window: &[Token],
    serves: bool,
    converted: &mut FileFacts,
) {
    let mut methods = Vec::<String>::new();
    let mut routes = Vec::<(String, SourceSpan)>::new();
    let mut index = 0_usize;
    while index < window.len() {
        let Some((property, literal, literal_index)) = comparison_at(window, text, index) else {
            index += 1;
            continue;
        };
        let value = unquote(literal.text(text));
        if property == "method"
            && HTTP_VERBS
                .iter()
                .any(|verb| value.eq_ignore_ascii_case(verb))
        {
            methods.push(value.to_ascii_uppercase());
        } else if PATH_PROPERTIES.contains(&property.as_str()) && value.starts_with('/') {
            routes.push((value.to_owned(), token_span(literal, text, path)));
        }
        index = literal_index + 1;
    }
    if routes.is_empty() || (methods.is_empty() && !serves) {
        return;
    }
    let verbs = if methods.is_empty() {
        vec!["ANY".to_owned()]
    } else {
        methods
    };
    for (route, span) in routes {
        for verb in &verbs {
            let name = format!("{verb} {route}");
            if converted
                .domains
                .iter()
                .any(|fact| fact.kind == NodeKind::Endpoint && fact.name == name)
            {
                continue;
            }
            converted.domains.push(DomainFact {
                name,
                kind: NodeKind::Endpoint,
                relation: EdgeKind::Exposes,
                span: span.clone(),
                owner: None,
            });
        }
    }
}

/// One equality between a property name and a string literal, in either
/// operand order. Returns the final property segment, the literal token, and
/// the index the caller resumes after. `!==`/`!=` and plain `=` never match:
/// the tokenizer emits punctuation one character at a time, so an equality is
/// exactly two or three consecutive `=` tokens with no leading `!`.
fn comparison_at<'tokens>(
    window: &'tokens [Token],
    text: &str,
    index: usize,
) -> Option<(String, &'tokens Token, usize)> {
    let token = window.get(index)?;
    if token.kind == TokenKind::Identifier {
        let cursor = equality_end(window, text, index + 1)?;
        let literal = window.get(cursor)?;
        (literal.kind == TokenKind::String).then(|| (token.text(text).to_owned(), literal, cursor))
    } else if token.kind == TokenKind::String {
        let cursor = equality_end(window, text, index + 1)?;
        let property = last_property_segment(window, text, cursor)?;
        Some((property.1, token, cursor.max(property.0)))
    } else {
        None
    }
}

fn equality_end(window: &[Token], text: &str, start: usize) -> Option<usize> {
    let mut cursor = start;
    while window
        .get(cursor)
        .is_some_and(|token| token.text(text) == "=")
    {
        cursor += 1;
    }
    (2..=3).contains(&(cursor - start)).then_some(cursor)
}

/// The last identifier of a `req.method`-style member chain starting at
/// `start`, with the index where the chain ends.
fn last_property_segment(window: &[Token], text: &str, start: usize) -> Option<(usize, String)> {
    let first = window.get(start)?;
    if first.kind != TokenKind::Identifier {
        return None;
    }
    let mut last = start;
    while window
        .get(last + 1)
        .is_some_and(|token| token.text(text) == ".")
        && window
            .get(last + 2)
            .is_some_and(|token| token.kind == TokenKind::Identifier)
    {
        last += 2;
    }
    Some((last, window[last].text(text).to_owned()))
}

/// Pages a desktop shell opens become imports of this file, so renderer code
/// is reachable from the process that loads it.
fn page_imports(path: &str, text: &str, window: &[Token], converted: &mut FileFacts) {
    for token in window {
        if token.kind != TokenKind::String {
            continue;
        }
        let value = unquote(token.text(text));
        let is_page = std::path::Path::new(value)
            .extension()
            .is_some_and(|extension| {
                extension.eq_ignore_ascii_case("html") || extension.eq_ignore_ascii_case("htm")
            });
        if !is_page || value.starts_with('/') {
            continue;
        }
        let specifier = if value.starts_with('.') {
            value.to_owned()
        } else {
            format!("./{value}")
        };
        if converted
            .imports
            .iter()
            .any(|item| item.target == specifier)
        {
            continue;
        }
        converted
            .imports
            .push(ImportFact::new(specifier, token_span(token, text, path)));
    }
}

fn opens_at(tokens: &[Token], text: &str, index: usize) -> Option<usize> {
    tokens
        .get(index)
        .is_some_and(|token| token.text(text) == "(")
        .then_some(index)
}

fn matching_paren(tokens: &[Token], text: &str, open: usize) -> Option<usize> {
    let mut depth = 0_usize;
    for (offset, token) in tokens.iter().enumerate().skip(open) {
        match token.text(text) {
            "(" => depth += 1,
            ")" => {
                depth = depth.checked_sub(1)?;
                if depth == 0 {
                    return Some(offset);
                }
            }
            _ => {}
        }
    }
    None
}

fn unquote(text: &str) -> &str {
    let mut value = text;
    for quote in ['"', '\'', '`'] {
        value = value
            .strip_prefix(quote)
            .and_then(|rest| rest.strip_suffix(quote))
            .unwrap_or(value);
    }
    value
}

fn token_span(token: &Token, text: &str, path: &str) -> SourceSpan {
    let width = u32::try_from(token.text(text).chars().count()).unwrap_or(0);
    SourceSpan::new(
        path,
        SourcePosition {
            line: token.line,
            column: token.column,
        },
        SourcePosition {
            line: token.line,
            column: token.column.saturating_add(width),
        },
    )
}
