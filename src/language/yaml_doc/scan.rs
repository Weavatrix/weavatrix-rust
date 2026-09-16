use crate::model::Diagnostic;

pub(super) const MAX_DEPTH: usize = 64;
pub(super) const MAX_NODES: usize = 20_000;

pub(super) struct Parser<'a> {
    pub raw: &'a str,
    pub pos: usize,
    pub nodes: usize,
}

impl Parser<'_> {
    pub(super) fn next_content_indent(&self) -> Option<(usize, usize)> {
        let mut index = self.pos;
        if index < self.raw.len() && !at_line_start(self.raw, index) {
            let line_end = line_end(self.raw, index);
            let rest = &self.raw[index..line_end];
            if !rest.trim().is_empty() && !rest.trim_start().starts_with('#') {
                return Some((column_at(self.raw, index), index));
            }
            index = if line_end < self.raw.len() {
                line_end + 1
            } else {
                return None;
            };
        }
        while index < self.raw.len() {
            let line_end = line_end(self.raw, index);
            let line = &self.raw[index..line_end];
            let trimmed = line.trim_start();
            if !trimmed.is_empty() && !trimmed.starts_with('#') {
                return Some((line.len() - trimmed.len(), index));
            }
            index = if line_end < self.raw.len() {
                line_end + 1
            } else {
                return None;
            };
        }
        None
    }

    pub(super) fn skip_separators(&mut self) {
        self.skip_blank();
        if self.starts_with("---") {
            self.pos += 3;
            self.skip_to_eol();
            self.consume('\n');
        }
    }

    pub(super) fn skip_blank(&mut self) {
        while let Some((indent, start)) = self.next_content_indent() {
            if start > self.pos {
                self.pos = start + indent;
                return;
            }
            if start == self.pos {
                if at_line_start(self.raw, start) {
                    self.pos = start + indent;
                }
                return;
            }
            self.pos = start + 1;
        }
        self.pos = self.raw.len();
    }

    pub(super) fn skip_to_content(&mut self) {
        if let Some((indent, start)) = self.next_content_indent() {
            self.pos = if at_line_start(self.raw, start) {
                start + indent
            } else {
                start
            };
        }
    }

    pub(super) fn skip_to_eol(&mut self) {
        if let Some(offset) = self.raw[self.pos..].find('\n') {
            self.pos += offset;
        } else {
            self.pos = self.raw.len();
        }
    }

    pub(super) fn skip_spaces(&mut self) {
        while self.peek() == Some(' ') || self.peek() == Some('\t') {
            self.pos += 1;
        }
    }

    pub(super) fn skip_bom(&mut self) {
        if self.raw.starts_with('\u{feff}') {
            self.pos = '\u{feff}'.len_utf8();
        }
    }

    pub(super) fn consume(&mut self, expected: char) -> bool {
        if self.peek() == Some(expected) {
            self.pos += expected.len_utf8();
            true
        } else {
            false
        }
    }

    pub(super) fn starts_with(&self, prefix: &str) -> bool {
        self.raw[self.pos..].starts_with(prefix)
    }

    pub(super) fn peek(&self) -> Option<char> {
        self.raw[self.pos..].chars().next()
    }

    pub(super) fn at_line_end(&self) -> bool {
        matches!(self.peek(), None | Some('\n' | '#'))
    }

    pub(super) fn line_tail_is_item(&self) -> bool {
        self.raw[self.pos..].starts_with("-\n") || self.raw[self.pos..] == *"-"
    }

    pub(super) fn looks_like_mapping_start(&self) -> bool {
        let line = self.raw[self.pos..].split('\n').next().unwrap_or("");
        let trimmed = line.trim_end();
        if trimmed.is_empty() {
            return false;
        }
        if trimmed.starts_with('\'') || trimmed.starts_with('"') {
            return trimmed.contains(':');
        }
        let Some((key, rest)) = trimmed.split_once(':') else {
            return false;
        };
        !key.is_empty() && (rest.is_empty() || rest.starts_with([' ', '\t']))
    }

    pub(super) fn budget(&mut self, depth: usize) -> Result<(), Diagnostic> {
        self.nodes += 1;
        if depth > MAX_DEPTH || self.nodes > MAX_NODES {
            return Err(self.error("", "YAML nesting or node budget exceeded"));
        }
        Ok(())
    }

    #[allow(clippy::unused_self)]
    pub(super) fn error(&self, path: &str, message: impl Into<String>) -> Diagnostic {
        Diagnostic {
            code: "yaml.limit".into(),
            message: if path.is_empty() {
                message.into()
            } else {
                format!("{path}: {}", message.into())
            },
            span: None,
        }
    }
}

fn at_line_start(raw: &str, index: usize) -> bool {
    index == 0 || raw.as_bytes().get(index.saturating_sub(1)) == Some(&b'\n')
}

fn line_end(raw: &str, index: usize) -> usize {
    raw[index..]
        .find('\n')
        .map_or(raw.len(), |offset| index + offset)
}

fn column_at(raw: &str, index: usize) -> usize {
    let prefix = &raw[..index];
    prefix
        .rsplit_once('\n')
        .map_or(prefix, |(_, last)| last)
        .chars()
        .count()
}
