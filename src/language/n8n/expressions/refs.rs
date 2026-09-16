use weavatrix_parse::{Language, Token, TokenKind, tokenize};

pub(super) struct NodeRef {
    pub name: String,
    pub selector: String,
    pub field: Option<String>,
    pub dynamic_key: bool,
}

pub(super) fn node_refs(expression: &str) -> Vec<NodeRef> {
    let tokens = tokenize(expression, Language::JavaScript);
    let mut refs = Vec::new();
    let mut index = 0;
    while index < tokens.len() {
        if tokens[index].is_trivia() || tokens[index].kind == TokenKind::String {
            index += 1;
            continue;
        }
        if let Some((reference, next)) = take_node_ref(expression, &tokens, index) {
            refs.push(reference);
            index = next;
            continue;
        }
        index += 1;
    }
    refs
}

pub(super) fn has_dynamic_node(expression: &str) -> bool {
    let tokens = tokenize(expression, Language::JavaScript);
    let mut index = 0;
    while index < tokens.len() {
        if tokens[index].is_trivia() || tokens[index].kind == TokenKind::String {
            index += 1;
            continue;
        }
        if tokens[index].text(expression) == "$"
            && let Some(open) = skip_trivia(&tokens, index + 1)
            && tokens[open].text(expression) == "("
        {
            if let Some(next) = skip_trivia(&tokens, open + 1)
                && tokens[next].kind == TokenKind::String
            {
                index += 1;
                continue;
            }
            return true;
        }
        index += 1;
    }
    false
}

pub(super) fn current_input(expression: &str) -> bool {
    mentions(expression, "$json") || mentions(expression, "$input")
}

pub(super) fn current_fields(expression: &str) -> Vec<String> {
    dotted_names(expression, "$json")
}

pub(super) fn env_or_var(expression: &str) -> Vec<String> {
    let mut names = dotted_names(expression, "$vars");
    names.extend(dotted_names(expression, "$env"));
    names
}

fn take_node_ref(source: &str, tokens: &[Token], start: usize) -> Option<(NodeRef, usize)> {
    let dollar = skip_trivia(tokens, start)?;
    if tokens[dollar].text(source) != "$" {
        return None;
    }
    let after_dollar = skip_trivia(tokens, dollar + 1)?;
    let text = tokens[after_dollar].text(source);
    let (name, cursor) = if text == "(" {
        let literal = skip_trivia(tokens, after_dollar + 1)?;
        if tokens[literal].kind != TokenKind::String {
            return None;
        }
        let close = skip_trivia(tokens, literal + 1)?;
        if tokens[close].text(source) != ")" {
            return None;
        }
        (unquote(tokens[literal].text(source))?, close + 1)
    } else if text == "node" || text == "items" {
        let open = skip_trivia(tokens, after_dollar + 1)?;
        let opener = tokens[open].text(source);
        if opener != "[" && opener != "(" {
            return None;
        }
        let literal = skip_trivia(tokens, open + 1)?;
        if tokens[literal].kind != TokenKind::String {
            return None;
        }
        let close = skip_trivia(tokens, literal + 1)?;
        let closer = tokens[close].text(source);
        if closer != "]" && closer != ")" {
            return None;
        }
        (unquote(tokens[literal].text(source))?, close + 1)
    } else {
        return None;
    };
    let (selector, field, dynamic_key, next) = trail(source, tokens, cursor);
    Some((
        NodeRef {
            name,
            selector,
            field,
            dynamic_key,
        },
        next,
    ))
}

fn trail(source: &str, tokens: &[Token], start: usize) -> (String, Option<String>, bool, usize) {
    let mut cursor = start;
    let mut selector = "node-ref".to_owned();
    let mut field = None;
    let mut dynamic_key = false;
    let mut seen_json = false;
    while let Some(index) = skip_trivia(tokens, cursor) {
        if tokens[index].text(source) != "." {
            break;
        }
        let Some(name_index) = skip_trivia(tokens, index + 1) else {
            break;
        };
        let name = tokens[name_index].text(source);
        cursor = name_index + 1;
        match name {
            "itemMatching" => selector = "itemMatching".into(),
            "first" => {
                selector = "first".into();
                cursor = skip_call(source, tokens, cursor);
            }
            "last" => {
                selector = "last".into();
                cursor = skip_call(source, tokens, cursor);
            }
            "all" => {
                selector = "all".into();
                cursor = skip_call(source, tokens, cursor);
            }
            "item" => selector = "linked-item".into(),
            "params" => selector = "params".into(),
            "isExecuted" => selector = "isExecuted".into(),
            "json" => {
                seen_json = true;
                if let Some(open) = skip_trivia(tokens, cursor)
                    && tokens[open].text(source) == "["
                {
                    dynamic_key = true;
                    cursor = skip_brackets(source, tokens, open);
                }
            }
            _ if seen_json && tokens[name_index].kind == TokenKind::Identifier => {
                field = Some(name.to_owned());
                seen_json = false;
            }
            _ => {}
        }
    }
    (selector, field, dynamic_key, cursor)
}

fn skip_call(source: &str, tokens: &[Token], start: usize) -> usize {
    let Some(open) = skip_trivia(tokens, start) else {
        return start;
    };
    if tokens[open].text(source) != "(" {
        return start;
    }
    skip_brackets(source, tokens, open)
}

fn skip_brackets(source: &str, tokens: &[Token], open: usize) -> usize {
    let opener = tokens[open].text(source);
    let closer = match opener {
        "(" => ")",
        "[" => "]",
        _ => return open + 1,
    };
    let mut depth = 1;
    let mut index = open + 1;
    while index < tokens.len() {
        if tokens[index].kind == TokenKind::String {
            index += 1;
            continue;
        }
        let text = tokens[index].text(source);
        if text == opener {
            depth += 1;
        } else if text == closer {
            depth -= 1;
            if depth == 0 {
                return index + 1;
            }
        }
        index += 1;
    }
    index
}

fn skip_trivia(tokens: &[Token], start: usize) -> Option<usize> {
    (start..tokens.len()).find(|&index| !tokens[index].is_trivia())
}

fn unquote(literal: &str) -> Option<String> {
    let bytes = literal.as_bytes();
    if bytes.len() < 2 {
        return None;
    }
    let quote = *bytes.first()?;
    if (quote != b'\'' && quote != b'"') || bytes.last() != Some(&quote) {
        return None;
    }
    let inner = &literal[1..literal.len() - 1];
    (!inner.is_empty()).then(|| inner.to_owned())
}

fn mentions(expression: &str, name: &str) -> bool {
    tokenize(expression, Language::JavaScript)
        .into_iter()
        .any(|token| {
            !token.is_trivia() && token.kind != TokenKind::String && token.text(expression) == name
        })
}

fn dotted_names(expression: &str, prefix: &str) -> Vec<String> {
    let tokens = tokenize(expression, Language::JavaScript);
    let mut names = Vec::new();
    let mut index = 0;
    while index < tokens.len() {
        if tokens[index].is_trivia() || tokens[index].kind == TokenKind::String {
            index += 1;
            continue;
        }
        if tokens[index].text(expression) == prefix
            && let Some(dot) = skip_trivia(&tokens, index + 1)
            && tokens[dot].text(expression) == "."
            && let Some(name) = skip_trivia(&tokens, dot + 1)
            && tokens[name].kind == TokenKind::Identifier
        {
            names.push(format!("{prefix}.{}", tokens[name].text(expression)));
            index = name + 1;
            continue;
        }
        index += 1;
    }
    names
}
