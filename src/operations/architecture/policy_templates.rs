//! `$n` group references for to-side path selectors (group matching).
//!
//! `toPath` and `toPathNot` may reference the capture groups of the same
//! rule's `fromPath` as `$1`..`$9`. Captured text is inserted as escaped
//! literal characters - a captured `.` matches a dot, not any character -
//! and an unmatched optional group inserts empty text. Validation compiles
//! the empty instantiation, so a template that would produce a pattern the
//! engine cannot evaluate is rejected before evaluation, and substituted
//! text is escaped literals only, which cannot change pattern structure.

use super::path_pattern::PathPattern;
use blazingly_json::Value;
use std::collections::BTreeMap;

pub(super) struct Target {
    segments: Vec<Segment>,
    max_ref: usize,
}

enum Segment {
    Literal(String),
    Reference(usize),
}

impl Target {
    pub(super) fn parse(pattern: &str) -> Self {
        let mut segments = Vec::new();
        let mut literal = String::new();
        let mut max_ref = 0;
        let mut characters = pattern.chars().peekable();
        while let Some(value) = characters.next() {
            let reference = (value == '$')
                .then(|| characters.peek().and_then(|next| "123456789".find(*next)))
                .flatten();
            let Some(index) = reference else {
                literal.push(value);
                continue;
            };
            characters.next();
            if !literal.is_empty() {
                segments.push(Segment::Literal(std::mem::take(&mut literal)));
            }
            max_ref = max_ref.max(index + 1);
            segments.push(Segment::Reference(index));
        }
        if !literal.is_empty() {
            segments.push(Segment::Literal(literal));
        }
        Self { segments, max_ref }
    }

    /// The highest referenced group, so `$2` needs two captured groups.
    pub(super) fn max_ref(&self) -> usize {
        self.max_ref
    }

    pub(super) fn instantiate(&self, captures: &[Option<String>]) -> String {
        self.segments
            .iter()
            .map(|segment| match segment {
                Segment::Literal(text) => text.clone(),
                Segment::Reference(index) => captures
                    .get(*index)
                    .and_then(Option::as_ref)
                    .map(|text| escaped(text))
                    .unwrap_or_default(),
            })
            .collect()
    }
}

fn escaped(text: &str) -> String {
    let mut output = String::with_capacity(text.len());
    for value in text.chars() {
        if !value.is_ascii_alphanumeric() {
            output.push('\\');
        }
        output.push(value);
    }
    output
}

pub(super) type TargetCache = BTreeMap<String, Option<PathPattern>>;

pub(super) struct TargetSelector {
    include: Option<TargetPattern>,
    exclude: Option<TargetPattern>,
}

enum TargetPattern {
    Fixed(PathPattern),
    Templated(Target),
}

impl TargetSelector {
    /// Returns `None` only for patterns `validate` already rejected.
    pub(super) fn compile(rule: &Value) -> Option<Self> {
        Some(Self {
            include: target_pattern(rule, "toPath").ok()?,
            exclude: target_pattern(rule, "toPathNot").ok()?,
        })
    }

    pub(super) fn selects(
        &self,
        path: &str,
        captures: &[Option<String>],
        cache: &mut TargetCache,
    ) -> bool {
        let included = match &self.include {
            None => true,
            Some(pattern) => matched(pattern, path, captures, cache),
        };
        let excluded = match &self.exclude {
            None => false,
            Some(pattern) => matched(pattern, path, captures, cache),
        };
        included && !excluded
    }
}

fn matched(
    pattern: &TargetPattern,
    path: &str,
    captures: &[Option<String>],
    cache: &mut TargetCache,
) -> bool {
    match pattern {
        TargetPattern::Fixed(compiled) => compiled.matches(path).is_some(),
        TargetPattern::Templated(template) => cache
            .entry(template.instantiate(captures))
            .or_insert_with_key(|key| PathPattern::compile(key).ok())
            .as_ref()
            .is_some_and(|compiled| compiled.matches(path).is_some()),
    }
}

/// `Err` marks a pattern `validate` already rejected; the selector is skipped.
fn target_pattern(rule: &Value, key: &str) -> Result<Option<TargetPattern>, ()> {
    let Some(raw) = rule.get(key) else {
        return Ok(None);
    };
    let Some(raw) = raw.as_str() else {
        return Err(());
    };
    let template = Target::parse(raw);
    if template.max_ref() == 0 {
        return match PathPattern::compile(raw) {
            Ok(compiled) => Ok(Some(TargetPattern::Fixed(compiled))),
            Err(_) => Err(()),
        };
    }
    Ok(Some(TargetPattern::Templated(template)))
}
