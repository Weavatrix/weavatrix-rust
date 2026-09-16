use super::lex::Scanner;
use super::model::{Diagram, Element, Relation};
use super::span::span_for;

pub(super) fn parse_statement(
    path: &str,
    raw: &str,
    scanner: &mut Scanner<'_>,
    diagram: &mut Diagram,
    group: Option<&String>,
) -> bool {
    let Some(mut sources) = parse_node_list(path, raw, scanner, diagram, group) else {
        return false;
    };
    while let Some(edge) = take_edge(scanner) {
        let Some(targets) = parse_node_list(path, raw, scanner, diagram, group) else {
            return false;
        };
        let occurrence = u32::try_from(diagram.relations.len()).unwrap_or(u32::MAX);
        let span = span_for(path, raw, edge.start, scanner.absolute());
        for from in &sources {
            for to in &targets {
                diagram.relations.push(Relation {
                    from: from.clone(),
                    to: to.clone(),
                    label: edge.label.clone(),
                    marker: edge.marker.clone(),
                    occurrence,
                    span: span.clone(),
                });
            }
        }
        sources = targets;
    }
    true
}

pub(super) fn take_until(scanner: &mut Scanner<'_>, close: char) -> Option<String> {
    let start = scanner.position();
    while !scanner.eof() {
        if scanner.peek_char() == Some('"') || scanner.peek_char() == Some('\'') {
            scanner.take_quoted()?;
            continue;
        }
        if scanner.peek_char() == Some(close) {
            let collected = scanner.slice(start, scanner.position()).to_owned();
            scanner.bump(close.len_utf8());
            while matches!(scanner.peek_char(), Some(']' | ')' | '}')) {
                scanner.bump(1);
            }
            return Some(trim_shape_label(&collected));
        }
        scanner.bump(scanner.peek_char()?.len_utf8());
    }
    None
}

fn parse_node_list(
    path: &str,
    raw: &str,
    scanner: &mut Scanner<'_>,
    diagram: &mut Diagram,
    group: Option<&String>,
) -> Option<Vec<String>> {
    let mut nodes = vec![parse_node(path, raw, scanner, diagram, group)?];
    while scanner.eat("&") {
        nodes.push(parse_node(path, raw, scanner, diagram, group)?);
    }
    Some(nodes)
}

fn parse_node(
    path: &str,
    raw: &str,
    scanner: &mut Scanner<'_>,
    diagram: &mut Diagram,
    group: Option<&String>,
) -> Option<String> {
    scanner.skip_trivia();
    let start = scanner.absolute();
    let id = scanner.take_ident()?;
    let label = take_shape(scanner).unwrap_or_else(|| id.clone());
    let span = span_for(path, raw, start, scanner.absolute());
    if let Some(existing) = diagram.elements.iter_mut().find(|element| element.id == id) {
        existing.occurrences.push(span);
        return Some(id);
    }
    diagram.elements.push(Element {
        id: id.clone(),
        label,
        span,
        group: group.cloned(),
        occurrences: Vec::new(),
    });
    Some(id)
}

struct EdgeTok {
    start: usize,
    label: String,
    marker: String,
}

fn take_edge(scanner: &mut Scanner<'_>) -> Option<EdgeTok> {
    scanner.skip_ws();
    let start = scanner.absolute();
    if scanner.eat("-->") {
        return Some(labeled(scanner, start, "arrow"));
    }
    if scanner.eat("-.->") || scanner.eat("-.-") {
        return Some(labeled(scanner, start, "dotted"));
    }
    if scanner.eat("==>") || scanner.eat("===") {
        return Some(labeled(scanner, start, "thick"));
    }
    if scanner.eat("---") || scanner.eat("--") {
        if scanner.eat(">") {
            return Some(labeled(scanner, start, "arrow"));
        }
        if let Some(label) = take_inline_label(scanner) {
            scanner.eat("-->");
            scanner.eat(">");
            return Some(EdgeTok {
                start,
                label,
                marker: "arrow".into(),
            });
        }
        return Some(EdgeTok {
            start,
            label: String::new(),
            marker: "line".into(),
        });
    }
    None
}

fn labeled(scanner: &mut Scanner<'_>, start: usize, marker: &str) -> EdgeTok {
    let label = if scanner.eat("|") {
        take_until(scanner, '|').unwrap_or_default()
    } else {
        String::new()
    };
    scanner.eat("|");
    EdgeTok {
        start,
        label,
        marker: marker.to_owned(),
    }
}

fn take_inline_label(scanner: &mut Scanner<'_>) -> Option<String> {
    scanner.skip_ws();
    if scanner.starts_with("-->") || scanner.starts_with(">") {
        return None;
    }
    let rest = scanner.rest();
    let end = rest.find("-->").or_else(|| rest.find('\n'))?;
    if end == 0 {
        return None;
    }
    let label = rest[..end].trim().to_owned();
    scanner.bump(end);
    Some(label)
}

fn take_shape(scanner: &mut Scanner<'_>) -> Option<String> {
    scanner.skip_ws();
    let open = scanner.peek_char()?;
    if !matches!(open, '[' | '(' | '{' | '>') {
        return None;
    }
    scanner.bump(open.len_utf8());
    while matches!(scanner.peek_char(), Some('[' | '(' | '{')) {
        scanner.bump(1);
    }
    let close = match open {
        '[' | '>' => ']',
        '(' => ')',
        '{' => '}',
        _ => return None,
    };
    take_until(scanner, close)
}

fn trim_shape_label(label: &str) -> String {
    label
        .trim()
        .trim_matches(|character| {
            matches!(character, '"' | '\'' | '[' | ']' | '(' | ')' | '{' | '}')
        })
        .to_owned()
}
