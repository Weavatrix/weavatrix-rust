pub(super) struct Scanner<'a> {
    text: &'a str,
    origin: usize,
    pos: usize,
}

impl<'a> Scanner<'a> {
    pub(super) fn new(text: &'a str, origin: usize) -> Self {
        Self {
            text,
            origin,
            pos: 0,
        }
    }

    pub(super) fn eof(&self) -> bool {
        self.pos >= self.text.len()
    }

    pub(super) fn origin_at(&self, local: usize) -> usize {
        self.origin.saturating_add(local)
    }

    pub(super) fn position(&self) -> usize {
        self.pos
    }

    pub(super) fn absolute(&self) -> usize {
        self.origin_at(self.pos)
    }

    pub(super) fn skip_trivia(&mut self) {
        loop {
            self.skip_ws();
            if self.starts_with("%%{") {
                self.skip_until("}%%");
                continue;
            }
            if self.starts_with("%%") {
                self.skip_line();
                continue;
            }
            break;
        }
    }

    pub(super) fn skip_ws(&mut self) {
        while let Some(character) = self.peek_char() {
            if character == '\n' || !character.is_whitespace() {
                break;
            }
            self.bump(character.len_utf8());
        }
    }

    pub(super) fn skip_line(&mut self) {
        if let Some(index) = self.rest().find('\n') {
            self.pos += index + 1;
        } else {
            self.pos = self.text.len();
        }
    }

    pub(super) fn skip_newline(&mut self) {
        if self.peek_char() == Some('\r') {
            self.bump(1);
        }
        if self.peek_char() == Some('\n') {
            self.bump(1);
        }
    }

    pub(super) fn take_ident(&mut self) -> Option<String> {
        self.skip_ws();
        if self.peek_char() == Some('"') || self.peek_char() == Some('\'') {
            return self.take_quoted();
        }
        let rest = self.rest();
        let mut chars = rest.chars();
        let first = chars.next()?;
        if !first.is_ascii_alphabetic() && first != '_' {
            return None;
        }
        let mut length = first.len_utf8();
        for character in chars {
            if character.is_ascii_alphanumeric() || matches!(character, '_' | '-') {
                length += character.len_utf8();
            } else {
                break;
            }
        }
        let ident = rest[..length].to_owned();
        self.pos += length;
        Some(ident)
    }

    pub(super) fn take_quoted(&mut self) -> Option<String> {
        let quote = self.peek_char().filter(|character| matches!(character, '"' | '\''))?;
        self.bump(1);
        let rest = self.rest();
        let mut escaped = false;
        let mut length = 0;
        for character in rest.chars() {
            length += character.len_utf8();
            if escaped {
                escaped = false;
                continue;
            }
            if character == '\\' {
                escaped = true;
                continue;
            }
            if character == quote {
                let inner = rest[..length.saturating_sub(1)].to_owned();
                self.pos += length;
                return Some(inner);
            }
        }
        None
    }

    pub(super) fn keyword(&mut self, name: &str) -> bool {
        self.skip_ws();
        if !self.starts_with(name) {
            return false;
        }
        let after = self.pos + name.len();
        let next = self.text[after..].chars().next();
        if next.is_some_and(|character| character.is_ascii_alphanumeric() || character == '_') {
            return false;
        }
        self.pos = after;
        true
    }

    pub(super) fn eat(&mut self, token: &str) -> bool {
        self.skip_ws();
        if self.starts_with(token) {
            self.pos += token.len();
            true
        } else {
            false
        }
    }

    pub(super) fn peek_char(&self) -> Option<char> {
        self.rest().chars().next()
    }

    pub(super) fn starts_with(&self, token: &str) -> bool {
        self.rest().starts_with(token)
    }

    pub(super) fn rest(&self) -> &'a str {
        &self.text[self.pos..]
    }

    pub(super) fn slice(&self, start: usize, end: usize) -> &'a str {
        let start = start.min(self.text.len());
        let end = end.min(self.text.len()).max(start);
        &self.text[start..end]
    }

    pub(super) fn bump(&mut self, amount: usize) {
        self.pos = (self.pos + amount).min(self.text.len());
    }

    fn skip_until(&mut self, token: &str) {
        if let Some(index) = self.rest().find(token) {
            self.pos += index + token.len();
        } else {
            self.skip_line();
        }
    }
}
