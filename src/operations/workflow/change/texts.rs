use crate::operations::node_path;
use std::collections::BTreeMap;
use weavatrix_graph::Node;

pub(super) struct FileTexts {
    pub base: BTreeMap<String, String>,
    pub head: BTreeMap<String, String>,
}

impl FileTexts {
    pub(super) fn new() -> Self {
        Self {
            base: BTreeMap::new(),
            head: BTreeMap::new(),
        }
    }

    pub(super) fn slice(&self, node: &Node, from_base: bool) -> Option<String> {
        let path = node_path(node)?;
        let source = if from_base {
            self.base.get(path)
        } else {
            self.head.get(path)
        }?;
        let span = node.span.as_ref()?;
        let start = usize::try_from(span.start.line.saturating_sub(1)).ok()?;
        let end = usize::try_from(span.end.line.max(span.start.line)).ok()?;
        Some(
            source
                .lines()
                .skip(start)
                .take(end.saturating_sub(start))
                .collect::<Vec<_>>()
                .join("\n"),
        )
    }
}

pub(super) fn code_only(text: &str) -> String {
    let mut out = String::new();
    let mut chars = text.chars().peekable();
    let mut in_string = None;
    while let Some(ch) = chars.next() {
        if let Some(quote) = in_string {
            out.push(ch);
            if ch == '\\' {
                if let Some(next) = chars.next() {
                    out.push(next);
                }
            } else if ch == quote {
                in_string = None;
            }
            continue;
        }
        if ch == '"' || ch == '\'' {
            in_string = Some(ch);
            out.push(ch);
            continue;
        }
        if ch == '/' && chars.peek() == Some(&'/') {
            while chars.next_if(|next| *next != '\n').is_some() {}
            continue;
        }
        if ch == '/' && chars.peek() == Some(&'*') {
            chars.next();
            while let Some(next) = chars.next() {
                if next == '*' && chars.peek() == Some(&'/') {
                    chars.next();
                    break;
                }
            }
            continue;
        }
        if ch == '#' {
            while chars.next_if(|next| *next != '\n').is_some() {}
            continue;
        }
        out.push(ch);
    }
    out.split_whitespace().collect::<Vec<_>>().join(" ")
}
