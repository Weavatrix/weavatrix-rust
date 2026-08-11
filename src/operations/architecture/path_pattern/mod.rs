//! Dependency-Cruiser-style path selectors, compiled without a regex crate.
//!
//! Contracts select files with a declared subset of regular-expression
//! syntax: `^` and `$` anchors, literal characters, `\` escapes of
//! punctuation, `.`, character classes such as `[^/]`, capturing and `(?:`
//! groups with `|` alternation, and the `*`, `+`, and `?` quantifiers.
//! A pattern outside this subset is rejected when the contract is validated:
//! a selector the engine cannot evaluate must fail loudly instead of
//! silently selecting nothing.

mod parser;
#[cfg(test)]
mod tests;

use std::collections::BTreeSet;

pub(super) struct PathPattern {
    program: Vec<Instruction>,
    groups: usize,
    anchored_end: bool,
}

enum Instruction {
    Char(Matcher),
    Save(usize),
    Split(usize, usize),
    Jump(usize),
    Done,
}

enum Matcher {
    Literal(char),
    Any,
    Class {
        negated: bool,
        singles: Vec<char>,
        ranges: Vec<(char, char)>,
    },
}

impl Matcher {
    fn accepts(&self, value: char) -> bool {
        match self {
            Self::Literal(expected) => value == *expected,
            Self::Any => true,
            Self::Class {
                negated,
                singles,
                ranges,
            } => {
                let inside = singles.contains(&value)
                    || ranges
                        .iter()
                        .any(|(low, high)| (*low..=*high).contains(&value));
                inside != *negated
            }
        }
    }
}

impl PathPattern {
    pub(super) fn compile(pattern: &str) -> Result<Self, String> {
        let compiled = parser::compile(pattern)?;
        Ok(Self {
            program: compiled.program,
            groups: compiled.groups,
            anchored_end: compiled.anchored_end,
        })
    }

    #[cfg(test)]
    pub(super) fn group_count(&self) -> usize {
        self.groups
    }

    /// Returns the captured groups of the leftmost match, or `None` when the
    /// path does not match. Slot 0 is the first capturing group.
    pub(super) fn matches(&self, path: &str) -> Option<Vec<Option<String>>> {
        let input: Vec<char> = path.chars().collect();
        let slots = 2 * (self.groups + 1);
        let mut pending = vec![(0_usize, 0_usize, vec![None; slots])];
        let mut visited = BTreeSet::new();
        while let Some((mut counter, mut position, mut saves)) = pending.pop() {
            loop {
                if !visited.insert((counter, position)) {
                    break;
                }
                match &self.program[counter] {
                    Instruction::Char(matcher) => {
                        let Some(value) = input.get(position) else {
                            break;
                        };
                        if !matcher.accepts(*value) {
                            break;
                        }
                        counter += 1;
                        position += 1;
                    }
                    Instruction::Save(slot) => {
                        saves[*slot] = Some(position);
                        counter += 1;
                    }
                    Instruction::Split(preferred, fallback) => {
                        pending.push((*fallback, position, saves.clone()));
                        counter = *preferred;
                    }
                    Instruction::Jump(target) => counter = *target,
                    Instruction::Done => {
                        if self.anchored_end && position != input.len() {
                            break;
                        }
                        return Some(self.captured(&input, &saves));
                    }
                }
            }
        }
        None
    }

    fn captured(&self, input: &[char], saves: &[Option<usize>]) -> Vec<Option<String>> {
        (1..=self.groups)
            .map(|group| {
                let start = saves.get(2 * group).copied().flatten()?;
                let end = saves.get(2 * group + 1).copied().flatten()?;
                Some(input.get(start..end)?.iter().collect())
            })
            .collect()
    }
}
