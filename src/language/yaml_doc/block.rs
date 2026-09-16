use super::scan::Parser;
use super::value::{Node, Scalar};
use crate::model::Diagnostic;

impl Parser<'_> {
    pub(super) fn parse_block_scalar(&mut self) -> Result<Node, Diagnostic> {
        let start = self.pos;
        let folded = self.peek() == Some('>');
        self.pos += 1;
        let mut chomp = Chomp::Clip;
        let mut indent_hint = None;
        loop {
            match self.peek() {
                Some('+') => {
                    chomp = Chomp::Keep;
                    self.pos += 1;
                }
                Some('-') => {
                    chomp = Chomp::Strip;
                    self.pos += 1;
                }
                Some(digit @ '1'..='9') if indent_hint.is_none() => {
                    indent_hint = Some(usize::from(digit as u8 - b'0'));
                    self.pos += 1;
                }
                Some(' ' | '\t') => self.pos += 1,
                Some('#') => {
                    self.skip_to_eol();
                    break;
                }
                Some('\n') | None => break,
                Some(_) => {
                    return Err(self.error("", "unsupported YAML block scalar header"));
                }
            }
        }
        self.consume('\n');
        let mut lines = Vec::<(usize, String, bool)>::new();
        let mut raw_end = self.pos;
        let detected = indent_hint
            .unwrap_or_else(|| self.next_content_indent().map_or(0, |(indent, _)| indent));
        while let Some((indent, line_start)) = self.next_content_indent() {
            if indent < detected
                && !self.raw[line_start..line_end(self.raw, line_start)]
                    .trim()
                    .is_empty()
            {
                break;
            }
            let line_end = line_end(self.raw, line_start);
            let content_start = line_start.saturating_add(detected.min(indent));
            let content = self.raw.get(content_start..line_end).unwrap_or("");
            lines.push((indent, content.to_owned(), content.is_empty()));
            raw_end = line_end;
            self.pos = if line_end < self.raw.len() {
                line_end + 1
            } else {
                line_end
            };
        }
        let mut decoded = if folded {
            fold_lines(&lines)
        } else {
            lines
                .iter()
                .map(|(_, content, _)| content.as_str())
                .collect::<Vec<_>>()
                .join("\n")
        };
        apply_chomp(&mut decoded, chomp);
        Ok(Node::Scalar(Scalar {
            decoded,
            raw_start: start,
            raw_end,
        }))
    }
}

#[derive(Clone, Copy)]
enum Chomp {
    Clip,
    Strip,
    Keep,
}

fn line_end(raw: &str, index: usize) -> usize {
    raw[index..]
        .find('\n')
        .map_or(raw.len(), |offset| index + offset)
}

fn fold_lines(lines: &[(usize, String, bool)]) -> String {
    let mut decoded = String::new();
    let mut pending_space = false;
    for (_, content, blank) in lines {
        if *blank {
            if !decoded.ends_with('\n') && !decoded.is_empty() {
                decoded.push('\n');
            }
            decoded.push('\n');
            pending_space = false;
            continue;
        }
        if pending_space && !decoded.ends_with('\n') && !decoded.is_empty() {
            decoded.push(' ');
        }
        decoded.push_str(content);
        pending_space = true;
    }
    decoded
}

fn apply_chomp(decoded: &mut String, chomp: Chomp) {
    match chomp {
        Chomp::Strip => {
            while decoded.ends_with('\n') {
                decoded.pop();
            }
        }
        Chomp::Clip => {
            while decoded.ends_with("\n\n") {
                decoded.pop();
            }
            if !decoded.is_empty() && !decoded.ends_with('\n') {
                decoded.push('\n');
            }
        }
        Chomp::Keep => {}
    }
}
