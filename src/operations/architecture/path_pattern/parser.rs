//! Recursive-descent compiler for the declared path-pattern subset.

use super::{Instruction, Matcher};

pub(super) struct Compiled {
    pub(super) program: Vec<Instruction>,
    pub(super) groups: usize,
    pub(super) anchored_end: bool,
}

pub(super) fn compile(pattern: &str) -> Result<Compiled, String> {
    let mut parser = Parser {
        pattern,
        characters: pattern.chars().collect(),
        position: 0,
        program: Vec::new(),
        groups: 0,
    };
    let anchored_start = parser.take('^');
    if !anchored_start {
        // An unanchored pattern may match anywhere; prefer the leftmost
        // start, so the skip branch is tried only after a match attempt.
        parser.program.push(Instruction::Split(3, 1));
        parser.program.push(Instruction::Char(Matcher::Any));
        parser.program.push(Instruction::Jump(0));
    }
    parser.program.push(Instruction::Save(0));
    parser.alternation(false)?;
    let anchored_end = parser.take_end_anchor();
    if parser.position < parser.characters.len() {
        return Err(parser.error("unbalanced `)`"));
    }
    parser.program.push(Instruction::Save(1));
    parser.program.push(Instruction::Done);
    Ok(Compiled {
        program: parser.program,
        groups: parser.groups,
        anchored_end,
    })
}

struct Parser<'pattern> {
    pattern: &'pattern str,
    characters: Vec<char>,
    position: usize,
    program: Vec<Instruction>,
    groups: usize,
}

impl Parser<'_> {
    fn alternation(&mut self, inside_group: bool) -> Result<(), String> {
        let mut branch_starts = vec![self.program.len()];
        self.program.push(Instruction::Split(0, 0));
        let mut branch_ends = Vec::new();
        self.concatenation()?;
        while self.take('|') {
            branch_ends.push(self.program.len());
            self.program.push(Instruction::Jump(0));
            branch_starts.push(self.program.len());
            self.program.push(Instruction::Split(0, 0));
            self.concatenation()?;
        }
        if inside_group && !self.take(')') {
            return Err(self.error("unbalanced `(`"));
        }
        let after = self.program.len();
        for (index, start) in branch_starts.iter().enumerate() {
            let body = start + 1;
            let fallback = branch_starts.get(index + 1).map_or(body, |next| *next);
            self.program[*start] = Instruction::Split(body, fallback);
        }
        for end in branch_ends {
            self.program[end] = Instruction::Jump(after);
        }
        Ok(())
    }

    fn concatenation(&mut self) -> Result<(), String> {
        while let Some(value) = self.peek() {
            if value == '|' || value == ')' {
                return Ok(());
            }
            if value == '$' && self.is_final_position() {
                return Ok(());
            }
            self.quantified()?;
        }
        Ok(())
    }

    fn quantified(&mut self) -> Result<(), String> {
        let entry = self.program.len();
        self.program.push(Instruction::Split(0, 0));
        let body = self.program.len();
        let quantifiable = self.atom()?;
        let quantifier = match self.peek() {
            Some(value @ ('*' | '+' | '?')) => {
                if !quantifiable {
                    return Err(self.error("a quantifier must follow a character, class, or group"));
                }
                self.position += 1;
                Some(value)
            }
            Some('{') => return Err(self.error("counted `{n,m}` quantifiers are not supported")),
            _ => None,
        };
        if quantifier.is_some() && matches!(self.peek(), Some('*' | '+' | '?')) {
            return Err(self.error("double quantifiers are not supported"));
        }
        match quantifier {
            Some('*') => {
                self.program.push(Instruction::Jump(entry));
                let after = self.program.len();
                self.program[entry] = Instruction::Split(body, after);
            }
            Some('+') => {
                // At least one occurrence: the entry always runs the body,
                // and each completed body chooses repeat-or-continue.
                let repeat = self.program.len();
                self.program.push(Instruction::Split(body, repeat + 1));
                self.program[entry] = Instruction::Split(body, body);
            }
            Some('?') => {
                let after = self.program.len();
                self.program[entry] = Instruction::Split(body, after);
            }
            _ => {
                self.program[entry] = Instruction::Split(body, body);
            }
        }
        Ok(())
    }

    /// Emits one atom and reports whether a quantifier may follow it.
    fn atom(&mut self) -> Result<bool, String> {
        let Some(value) = self.advance() else {
            return Err(self.error("a pattern element is missing"));
        };
        match value {
            '(' => self.group().map(|()| true),
            '[' => self.class().map(|()| true),
            '\\' => {
                let matcher = self.escape()?;
                self.program.push(Instruction::Char(matcher));
                Ok(true)
            }
            '.' => {
                self.program.push(Instruction::Char(Matcher::Any));
                Ok(true)
            }
            '^' => Err(self.error("`^` is supported only as the leading anchor")),
            '$' => Err(self.error("`$` is supported only as the trailing anchor")),
            '*' | '+' | '?' => {
                Err(self.error("a quantifier must follow a character, class, or group"))
            }
            '{' | '}' => Err(self.error("counted `{n,m}` quantifiers are not supported")),
            other => {
                self.program
                    .push(Instruction::Char(Matcher::Literal(other)));
                Ok(true)
            }
        }
    }

    fn group(&mut self) -> Result<(), String> {
        if self.peek() == Some('?') {
            if self.peek_at(1) == Some(':') {
                self.position += 2;
                return self.alternation(true);
            }
            return Err(self.error("only plain `(` and `(?:` groups are supported"));
        }
        self.groups += 1;
        let group = self.groups;
        self.program.push(Instruction::Save(2 * group));
        self.alternation(true)?;
        self.program.push(Instruction::Save(2 * group + 1));
        Ok(())
    }

    fn class(&mut self) -> Result<(), String> {
        let negated = self.take('^');
        let mut singles = Vec::new();
        let mut ranges = Vec::new();
        loop {
            let Some(value) = self.advance() else {
                return Err(self.error("unbalanced `[`"));
            };
            let item = match value {
                ']' if singles.is_empty() && ranges.is_empty() => {
                    return Err(self.error("an empty character class matches nothing"));
                }
                ']' => break,
                '\\' => self.class_literal()?,
                other => other,
            };
            if self.peek() == Some('-') && self.peek_at(1) != Some(']') && self.peek_at(1).is_some()
            {
                self.position += 1;
                let Some(end) = self.advance() else {
                    return Err(self.error("unbalanced `[`"));
                };
                let end = if end == '\\' {
                    self.class_literal()?
                } else {
                    end
                };
                if end < item {
                    return Err(self.error("a character range must be ascending"));
                }
                ranges.push((item, end));
            } else {
                singles.push(item);
            }
        }
        self.program.push(Instruction::Char(Matcher::Class {
            negated,
            singles,
            ranges,
        }));
        Ok(())
    }

    fn class_literal(&mut self) -> Result<char, String> {
        match self.escape()? {
            Matcher::Literal(literal) => Ok(literal),
            _ => Err(self.error("unsupported escape inside a character class")),
        }
    }

    fn escape(&mut self) -> Result<Matcher, String> {
        let Some(value) = self.advance() else {
            return Err(self.error("a trailing `\\` escapes nothing"));
        };
        if value.is_ascii_alphanumeric() {
            return Err(self.error(
                "shorthand classes and backreferences such as `\\d` or `\\1` are not supported",
            ));
        }
        Ok(Matcher::Literal(value))
    }

    fn take_end_anchor(&mut self) -> bool {
        if self.peek() == Some('$') && self.is_final_position() {
            self.position += 1;
            return true;
        }
        false
    }

    fn is_final_position(&self) -> bool {
        self.position + 1 == self.characters.len()
    }

    fn take(&mut self, expected: char) -> bool {
        if self.peek() == Some(expected) {
            self.position += 1;
            return true;
        }
        false
    }

    fn peek(&self) -> Option<char> {
        self.peek_at(0)
    }

    fn peek_at(&self, offset: usize) -> Option<char> {
        self.characters.get(self.position + offset).copied()
    }

    fn advance(&mut self) -> Option<char> {
        let value = self.peek();
        if value.is_some() {
            self.position += 1;
        }
        value
    }

    fn error(&self, reason: &str) -> String {
        format!("path pattern `{}` is not supported: {reason}", self.pattern)
    }
}
