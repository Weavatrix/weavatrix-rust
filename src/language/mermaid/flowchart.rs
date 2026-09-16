use super::lex::Scanner;
use super::model::{Completeness, Diagram, Group, MAX_EDGES, MAX_NODES, Region};
use super::span::{file_span, span_for};
use crate::model::Diagnostic;

pub(super) fn parse(path: &str, raw: &str, region: &Region) -> Diagram {
    let text = &raw[region.start..region.end];
    let mut scanner = Scanner::new(text, region.start);
    let mut diagram = Diagram {
        key: format!("{path}#{}", region.index),
        direction: String::new(),
        kind: "flowchart".into(),
        completeness: Completeness::Full,
        span: span_for(path, raw, region.start, region.end),
        elements: Vec::new(),
        groups: Vec::new(),
        relations: Vec::new(),
        diagnostics: Vec::new(),
    };
    scanner.skip_trivia();
    if !parse_header(&mut scanner, &mut diagram) {
        note(
            &mut diagram,
            path,
            raw,
            scanner.absolute(),
            "mermaid.unsupported",
            "diagram is missing a flowchart or graph header",
        );
    }
    let mut groups = Vec::<String>::new();
    while !scanner.eof() {
        scanner.skip_trivia();
        if scanner.eof() {
            break;
        }
        if scanner.peek_char() == Some('\n') {
            scanner.skip_newline();
            continue;
        }
        if skip_directive(&mut scanner) {
            continue;
        }
        if scanner.keyword("end") {
            groups.pop();
            continue;
        }
        if scanner.keyword("subgraph") {
            if let Some(group) = parse_subgraph(path, raw, &mut scanner) {
                groups.push(group.id.clone());
                diagram.groups.push(group);
            } else {
                mark_partial(
                    &mut diagram,
                    path,
                    raw,
                    scanner.absolute(),
                    "invalid subgraph",
                );
            }
            continue;
        }
        if !super::edges::parse_statement(path, raw, &mut scanner, &mut diagram, groups.last()) {
            mark_partial(
                &mut diagram,
                path,
                raw,
                scanner.absolute(),
                "unsupported flowchart statement",
            );
            scanner.skip_line();
        }
        if diagram.elements.len() > MAX_NODES || diagram.relations.len() > MAX_EDGES {
            note(
                &mut diagram,
                path,
                raw,
                scanner.absolute(),
                "mermaid.limit",
                "diagram analysis stopped at a declared limit",
            );
            diagram.completeness = Completeness::Partial;
            break;
        }
    }
    diagram
}

fn parse_header(scanner: &mut Scanner<'_>, diagram: &mut Diagram) -> bool {
    if scanner.keyword("flowchart") {
        diagram.kind = "flowchart".into();
    } else if scanner.keyword("graph") {
        diagram.kind = "graph".into();
    } else {
        return false;
    }
    scanner.skip_ws();
    if let Some(direction) = scanner.take_ident()
        && matches!(direction.as_str(), "TB" | "TD" | "BT" | "RL" | "LR")
    {
        diagram.direction = direction;
    }
    true
}

fn skip_directive(scanner: &mut Scanner<'_>) -> bool {
    if scanner.keyword("classDef")
        || scanner.keyword("class")
        || scanner.keyword("style")
        || scanner.keyword("linkStyle")
        || scanner.keyword("click")
        || scanner.keyword("direction")
    {
        scanner.skip_line();
        true
    } else {
        false
    }
}

fn parse_subgraph(path: &str, raw: &str, scanner: &mut Scanner<'_>) -> Option<Group> {
    let start = scanner.absolute();
    let id = scanner.take_ident()?;
    scanner.skip_ws();
    let title = if scanner.eat("[") {
        let title = super::edges::take_until(scanner, ']')?;
        scanner.eat("]");
        title
    } else {
        id.clone()
    };
    Some(Group {
        id,
        title,
        span: span_for(path, raw, start, scanner.absolute()),
    })
}

fn mark_partial(diagram: &mut Diagram, path: &str, raw: &str, start: usize, message: &str) {
    diagram.completeness = Completeness::Partial;
    note(diagram, path, raw, start, "mermaid.unsupported", message);
}

fn note(diagram: &mut Diagram, path: &str, raw: &str, start: usize, code: &str, message: &str) {
    diagram.diagnostics.push(Diagnostic {
        code: code.into(),
        message: message.into(),
        span: Some(if start < raw.len() {
            span_for(path, raw, start, start.saturating_add(1))
        } else {
            file_span(path)
        }),
    });
}
