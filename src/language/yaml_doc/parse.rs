use super::scan::Parser;
use super::value::{Node, Scalar};
use crate::model::Diagnostic;

pub(crate) fn parse(path: &str, raw: &str) -> Result<Vec<Node>, Diagnostic> {
    let mut parser = Parser {
        raw,
        pos: 0,
        nodes: 0,
    };
    parser.skip_bom();
    let mut documents = Vec::new();
    while parser.pos < raw.len() {
        parser.skip_separators();
        if parser.pos >= raw.len() {
            break;
        }
        documents.push(parser.parse_block(0, 0)?);
        parser.skip_blank();
    }
    if documents.is_empty() {
        return Err(parser.error(path, "yaml document is empty"));
    }
    Ok(documents)
}

impl Parser<'_> {
    fn parse_block(&mut self, indent: usize, depth: usize) -> Result<Node, Diagnostic> {
        self.budget(depth)?;
        self.skip_blank();
        let start = self.pos;
        if self.starts_with("- ") || self.peek() == Some('-') && self.line_tail_is_item() {
            return self.parse_sequence(indent, depth, start);
        }
        self.parse_mapping(indent, depth, start)
    }

    fn parse_mapping(
        &mut self,
        indent: usize,
        depth: usize,
        start: usize,
    ) -> Result<Node, Diagnostic> {
        let mut entries: Vec<(Scalar, Node)> = Vec::new();
        while let Some((line_indent, _)) = self.next_content_indent() {
            if line_indent < indent {
                break;
            }
            if line_indent > indent && entries.is_empty() {
                return Err(self.error("", "unexpected indented mapping"));
            }
            if self.starts_with("- ") {
                break;
            }
            let key = self.parse_key()?;
            if entries
                .iter()
                .any(|(existing, _)| existing.decoded == key.decoded)
            {
                return Err(self.error("", format!("duplicate YAML key {}", key.decoded)));
            }
            self.skip_spaces();
            if !self.consume(':') {
                return Err(self.error("", "expected ':' after mapping key"));
            }
            self.skip_spaces();
            let value = self.parse_value(indent, depth)?;
            entries.push((key, value));
            self.skip_blank();
        }
        Ok(Node::Mapping {
            start,
            end: self.pos.max(start + 1),
            entries,
        })
    }

    fn parse_sequence(
        &mut self,
        indent: usize,
        depth: usize,
        start: usize,
    ) -> Result<Node, Diagnostic> {
        let mut items = Vec::new();
        while let Some((line_indent, _)) = self.next_content_indent() {
            if line_indent < indent || !self.starts_with("- ") && self.peek() != Some('-') {
                break;
            }
            if !self.consume('-') {
                break;
            }
            self.skip_spaces();
            items.push(self.parse_value(indent, depth)?);
            self.skip_blank();
        }
        Ok(Node::Sequence {
            start,
            end: self.pos.max(start + 1),
            items,
        })
    }

    fn parse_value(&mut self, parent_indent: usize, depth: usize) -> Result<Node, Diagnostic> {
        self.budget(depth)?;
        if self.starts_with("! ")
            || self.starts_with("!!")
            || self.peek() == Some('&')
            || self.peek() == Some('*')
        {
            return Err(self.error("", "YAML tags and aliases are not executed"));
        }
        if self.starts_with("|") || self.starts_with(">") {
            return Ok(self.parse_block_scalar());
        }
        if self.starts_with("[]") {
            let start = self.pos;
            self.pos += 2;
            return Ok(Node::Sequence {
                items: Vec::new(),
                start,
                end: self.pos,
            });
        }
        if self.starts_with("{}") {
            let start = self.pos;
            self.pos += 2;
            return Ok(Node::Mapping {
                entries: Vec::new(),
                start,
                end: self.pos,
            });
        }
        if self.peek() == Some('[') || self.peek() == Some('{') {
            return Err(self.error("", "flow YAML collections are not admitted"));
        }
        if self.at_line_end() {
            self.skip_blank();
            let Some((child_indent, _)) = self.next_content_indent() else {
                return Ok(self.empty_scalar());
            };
            if self.starts_with("- ") || self.peek() == Some('-') && self.line_tail_is_item() {
                if child_indent >= parent_indent {
                    return self.parse_sequence(child_indent, depth + 1, self.pos);
                }
            } else if child_indent <= parent_indent {
                return Ok(self.empty_scalar());
            }
            return self.parse_block(child_indent, depth + 1);
        }
        if self.looks_like_mapping_start() {
            let indent = self
                .next_content_indent()
                .map_or(0, |(line_indent, _)| line_indent);
            return self.parse_mapping(indent, depth + 1, self.pos);
        }
        self.parse_flow_scalar()
    }

    fn parse_key(&mut self) -> Result<Scalar, Diagnostic> {
        self.skip_to_content();
        if self.peek() == Some('\'') || self.peek() == Some('"') {
            return self.parse_quoted();
        }
        let start = self.pos;
        while let Some(character) = self.peek() {
            if character == ':' || character == '#' || character == '\n' {
                break;
            }
            self.pos += character.len_utf8();
        }
        let raw = self.raw[start..self.pos].trim_end();
        Ok(Scalar {
            decoded: raw.to_owned(),
            raw_start: start,
            raw_end: start + raw.len(),
        })
    }

    fn parse_flow_scalar(&mut self) -> Result<Node, Diagnostic> {
        if self.peek() == Some('\'') || self.peek() == Some('"') {
            return Ok(Node::Scalar(self.parse_quoted()?));
        }
        let start = self.pos;
        while let Some(character) = self.peek() {
            if character == '\n' || character == '#' {
                break;
            }
            self.pos += character.len_utf8();
        }
        let raw = self.raw[start..self.pos].trim_end();
        Ok(Node::Scalar(Scalar {
            decoded: raw.to_owned(),
            raw_start: start,
            raw_end: start + raw.len(),
        }))
    }

    fn parse_quoted(&mut self) -> Result<Scalar, Diagnostic> {
        let quote = self
            .peek()
            .ok_or_else(|| self.error("", "unterminated scalar"))?;
        let start = self.pos;
        self.pos += 1;
        let mut decoded = String::new();
        while let Some(character) = self.peek() {
            self.pos += character.len_utf8();
            if character == quote {
                if quote == '\'' && self.peek() == Some('\'') {
                    decoded.push('\'');
                    self.pos += 1;
                    continue;
                }
                return Ok(Scalar {
                    decoded,
                    raw_start: start,
                    raw_end: self.pos,
                });
            }
            if quote == '"' && character == '\\' {
                decoded.push(self.take_escape()?);
                continue;
            }
            decoded.push(character);
        }
        Err(self.error("", "unterminated quoted scalar"))
    }

    fn parse_block_scalar(&mut self) -> Node {
        let start = self.pos;
        self.pos += 1;
        self.skip_to_eol();
        self.consume('\n');
        let mut decoded = String::new();
        let mut raw_end = self.pos;
        let body_indent = self.next_content_indent().map_or(0, |(indent, _)| indent);
        while let Some((indent, line_start)) = self.next_content_indent() {
            if indent < body_indent {
                break;
            }
            self.pos = line_start + body_indent;
            let line_start_pos = self.pos;
            self.skip_to_eol();
            if !decoded.is_empty() {
                decoded.push('\n');
            }
            decoded.push_str(&self.raw[line_start_pos..self.pos]);
            raw_end = self.pos;
            self.consume('\n');
        }
        Node::Scalar(Scalar {
            decoded,
            raw_start: start,
            raw_end,
        })
    }

    fn take_escape(&mut self) -> Result<char, Diagnostic> {
        let Some(character) = self.peek() else {
            return Err(self.error("", "unterminated escape"));
        };
        self.pos += character.len_utf8();
        Ok(match character {
            'n' => '\n',
            'r' => '\r',
            't' => '\t',
            '\\' | '"' | '/' => character,
            'u' => self.take_hex_escape()?,
            other => other,
        })
    }

    fn take_hex_escape(&mut self) -> Result<char, Diagnostic> {
        let start = self.pos;
        if self.pos + 4 > self.raw.len() {
            return Err(self.error("", "truncated \\u escape"));
        }
        self.pos += 4;
        let value = u32::from_str_radix(&self.raw[start..self.pos], 16)
            .map_err(|_| self.error("", "invalid \\u escape"))?;
        char::from_u32(value).ok_or_else(|| self.error("", "invalid \\u scalar"))
    }

    fn empty_scalar(&self) -> Node {
        Node::Scalar(Scalar {
            decoded: String::new(),
            raw_start: self.pos,
            raw_end: self.pos,
        })
    }
}
