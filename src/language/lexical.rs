mod declaration;
mod domain;
mod support;

use super::{
    FileFacts, ImportFact, Language, LanguageAdapter, MountFact, ReferenceFact, SourceFile,
    SymbolFact, SymbolLocator,
};
use crate::Result;
use declaration::{declaration, inheritance};
use domain::{
    domain_facts, object_route_key, object_route_method, parse_sql, parse_yaml, route_fact,
};
use std::collections::BTreeMap;
use support::{brace_delta, control_word, is_ident, line_number, line_span, sort_facts};
use weavatrix_graph::{EdgeKind, NodeKind};

#[derive(Debug, Clone)]
pub struct LexicalAdapter {
    language: Language,
    extensions: &'static [&'static str],
}

impl LexicalAdapter {
    pub fn defaults() -> impl Iterator<Item = Self> {
        [
            Self::new(Language::Go, &["go"]),
            Self::new(Language::C, &["c", "h"]),
            Self::new(Language::Cpp, &["cc", "cpp", "cxx", "hh", "hpp", "hxx"]),
            Self::new(Language::Bash, &["sh", "bash", "zsh"]),
            Self::new(Language::Sql, &["sql", "psql"]),
            Self::new(Language::Kubernetes, &["yaml", "yml"]),
            Self::new(Language::JavaScript, &["js", "jsx", "mjs", "cjs"]),
            Self::new(Language::TypeScript, &["ts", "tsx", "mts", "cts"]),
            Self::new(Language::Python, &["py", "pyi"]),
            Self::new(Language::Java, &["java"]),
            Self::new(Language::CSharp, &["cs"]),
        ]
        .into_iter()
    }

    const fn new(language: Language, extensions: &'static [&'static str]) -> Self {
        Self {
            language,
            extensions,
        }
    }
}

impl LanguageAdapter for LexicalAdapter {
    fn language(&self) -> Language {
        self.language.clone()
    }

    fn extensions(&self) -> &'static [&'static str] {
        self.extensions
    }

    fn extractor(&self) -> &'static str {
        "weavatrix.rust.lexical"
    }

    fn parse(&self, source: SourceFile<'_>) -> Result<FileFacts> {
        if self.language == Language::Sql {
            return Ok(parse_sql(&source));
        }
        if self.language == Language::Kubernetes {
            return Ok(parse_yaml(&source));
        }
        Ok(parse_code(&source, &self.language))
    }
}

fn parse_code(source: &SourceFile<'_>, language: &Language) -> FileFacts {
    let mut facts = FileFacts::default();
    let mut owners = Vec::<OwnerScope>::new();
    let mut object_route = None::<String>;
    let route_objects = matches!(language, Language::JavaScript | Language::TypeScript)
        && !source.path.contains("openapi");
    let mut depth = 0_i32;
    let mut go_block = None::<GoBlock>;
    let script = matches!(language, Language::JavaScript | Language::TypeScript);
    let mut bindings = BTreeMap::<String, String>::new();
    let mut raw_mounts = Vec::<(String, MountRef)>::new();
    let mut pending_route = None::<&'static str>;
    for (offset, raw) in source.text.lines().enumerate() {
        let line_number = line_number(offset);
        let line = raw.trim();
        if line.is_empty() || line.starts_with("//") || line.starts_with('#') {
            continue;
        }
        discard_completed_scopes(&mut owners, language, raw, depth);
        let span = line_span(source.path, line_number, raw);
        if *language == Language::Go && go_group(line, &mut go_block, &mut facts, &span, &owners) {
            continue;
        }
        let declaration =
            declaration(line, language).filter(|(_, kind)| !scoped_out(kind, language, &owners));
        if let Some((name, kind)) = declaration.as_ref() {
            let locator = SymbolLocator {
                name: name.clone(),
                kind: kind.clone(),
                span: span.clone(),
            };
            facts.symbols.push(SymbolFact {
                name: name.clone(),
                kind: kind.clone(),
                span: span.clone(),
            });
            if owns_scope(kind, line, language) {
                owners.push(OwnerScope {
                    locator,
                    boundary: scope_boundary(language, raw, depth),
                });
            }
        }
        let owner = owners.last().map(|scope| scope.locator.clone());
        if script
            && let Some(method) = pending_route.take()
            && line.starts_with(['\'', '"', '`'])
            && let Some(path) = quoted_segment(line)
            && path.starts_with('/')
        {
            facts
                .domains
                .push(route_fact(method, &path, &span, owner.clone()));
        }
        if route_objects {
            track_object_routes(line, &span, owner.as_ref(), &mut object_route, &mut facts);
        }
        for (name, kind) in inheritance(line, language) {
            facts.references.push(ReferenceFact {
                name,
                kind,
                span: span.clone(),
                owner: owner.clone(),
            });
        }
        for target in import_targets(line, language) {
            facts.imports.push(ImportFact {
                target,
                span: span.clone(),
            });
        }
        if script {
            collect_reexport(line, &span, &mut facts);
        }
        for name in call_names(line) {
            if declaration.as_ref().is_some_and(|item| item.0 == name) {
                continue;
            }
            facts.references.push(ReferenceFact {
                name,
                kind: EdgeKind::Calls,
                span: span.clone(),
                owner: owner.clone(),
            });
        }
        facts
            .domains
            .extend(domain_facts(line, &span, owner.as_ref()));
        if script {
            collect_script_bindings(line, &mut bindings);
            collect_script_mounts(line, &mut raw_mounts);
            pending_route = route_method_opening(line);
        }
        depth += brace_delta(line);
    }
    finalize_mounts(raw_mounts, &bindings, &mut facts);
    sort_facts(&mut facts);
    facts
}

/// `export { x } from './y'` and `export * from './y'` forward another
/// module's surface to importers of this file.
fn collect_reexport(line: &str, span: &weavatrix_graph::SourceSpan, facts: &mut FileFacts) {
    if let Some(value) = line.strip_prefix("export ")
        && value.contains(" from ")
        && let Some(target) = quoted_segment(value)
    {
        facts.reexports.push(ImportFact {
            target,
            span: span.clone(),
        });
    }
}

fn finalize_mounts(
    raw_mounts: Vec<(String, MountRef)>,
    bindings: &BTreeMap<String, String>,
    facts: &mut FileFacts,
) {
    for (prefix, target) in raw_mounts {
        let specifier = match target {
            MountRef::Specifier(specifier) => Some(specifier),
            MountRef::Binding(name) => bindings.get(&name).cloned(),
        };
        if let Some(target) = specifier {
            facts.mounts.push(MountFact { prefix, target });
        }
    }
}

/// Tracks `{ '/path': { GET: handler } }` route-table objects.
fn track_object_routes(
    line: &str,
    span: &weavatrix_graph::SourceSpan,
    owner: Option<&SymbolLocator>,
    object_route: &mut Option<String>,
    facts: &mut FileFacts,
) {
    if let Some((path, block)) = object_route_key(line) {
        if block {
            *object_route = Some(path);
        } else {
            facts
                .domains
                .push(route_fact("ANY", &path, span, owner.cloned()));
            *object_route = None;
        }
    } else if let Some(method) = object_route_method(line)
        && let Some(path) = &*object_route
    {
        facts
            .domains
            .push(route_fact(method, path, span, owner.cloned()));
    } else if line.starts_with('}') {
        *object_route = None;
    }
}

/// A route registration whose path argument continues on the next line.
fn route_method_opening(line: &str) -> Option<&'static str> {
    [
        (".get(", "GET"),
        (".post(", "POST"),
        (".put(", "PUT"),
        (".patch(", "PATCH"),
        (".delete(", "DELETE"),
    ]
    .into_iter()
    .find_map(|(suffix, method)| line.ends_with(suffix).then_some(method))
}

#[derive(Debug, Clone)]
enum MountRef {
    /// `use(p, require('./x'))` names the module inline.
    Specifier(String),
    /// `use(p, router)` references a binding declared elsewhere in the file.
    Binding(String),
}

/// Records `const x = require('spec')`, destructured
/// `const { a, b: c } = require('spec')`, and default/namespace
/// `import x from 'spec'` bindings usable as mount targets.
fn collect_script_bindings(line: &str, bindings: &mut BTreeMap<String, String>) {
    for prefix in ["const ", "let ", "var "] {
        if let Some(rest) = line.strip_prefix(prefix) {
            let Some((left, right)) = rest.split_once('=') else {
                return;
            };
            let Some(after) = right.split_once("require(") else {
                return;
            };
            let Some(specifier) = quoted_segment(after.1) else {
                return;
            };
            let left = left.trim();
            if let Some(inner) = left.strip_prefix('{') {
                for part in inner.trim_end_matches(['}', ' ']).split(',') {
                    let local = part.split(':').next_back().unwrap_or(part);
                    let name = support::identifier(local);
                    if !name.is_empty() {
                        bindings.insert(name.to_owned(), specifier.clone());
                    }
                }
            } else {
                let name = support::identifier(left);
                if !name.is_empty() {
                    bindings.insert(name.to_owned(), specifier);
                }
            }
            return;
        }
    }
    let Some(rest) = line.strip_prefix("import ") else {
        return;
    };
    let rest = rest.trim_start();
    let rest = rest.strip_prefix("* as ").unwrap_or(rest);
    let name = support::identifier(rest);
    if name.is_empty() || !rest.contains(" from ") {
        return;
    }
    if let Some(specifier) = quoted_segment(rest) {
        bindings.insert(name.to_owned(), specifier);
    }
}

/// Records `X.use('/prefix', target)` router mounts on the line.
fn collect_script_mounts(line: &str, mounts: &mut Vec<(String, MountRef)>) {
    let mut rest = line;
    while let Some(position) = rest.find(".use(") {
        let after = &rest[position + 5..];
        let argument = after.trim_start();
        let (prefix, target_text) = match argument.chars().next() {
            Some(quote @ ('"' | '\'' | '`')) => {
                let body = &argument[1..];
                let Some(end) = body.find(quote) else {
                    rest = after;
                    continue;
                };
                let remainder = body[end + 1..].trim_start().trim_start_matches(',');
                (body[..end].to_owned(), remainder.trim_start())
            }
            _ => (String::new(), argument),
        };
        if prefix.is_empty() || prefix.starts_with('/') {
            // Middleware may sit between the prefix and the router; treat
            // every argument as a candidate - non-routers add no endpoints.
            for part in target_text.split(',').take(4) {
                if let Some(target) = mount_target(part.trim()) {
                    mounts.push((prefix.clone(), target));
                }
            }
        }
        rest = after;
    }
}

fn mount_target(text: &str) -> Option<MountRef> {
    if let Some(rest) = text.strip_prefix("require(") {
        return quoted_segment(rest).map(MountRef::Specifier);
    }
    let name = support::identifier(text);
    (!name.is_empty()).then(|| MountRef::Binding(name.to_owned()))
}

#[derive(Debug, Clone)]
struct OwnerScope {
    locator: SymbolLocator,
    boundary: ScopeBoundary,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum GoBlock {
    Imports,
    Constants,
    Variables,
}

/// Tracks Go `import (` / `const (` / `var (` groups and records their
/// members; returns true when the line belongs to such a group.
fn go_group(
    line: &str,
    block: &mut Option<GoBlock>,
    facts: &mut FileFacts,
    span: &weavatrix_graph::SourceSpan,
    owners: &[OwnerScope],
) -> bool {
    if let Some(active) = *block {
        if line.starts_with(')') {
            *block = None;
            return true;
        }
        match active {
            GoBlock::Imports => {
                if let Some(target) = quoted_segment(line) {
                    facts.imports.push(ImportFact {
                        target,
                        span: span.clone(),
                    });
                }
            }
            GoBlock::Constants | GoBlock::Variables => {
                let name = support::identifier(line);
                if !name.is_empty() && owners.is_empty() && !control_word(name) {
                    facts.symbols.push(SymbolFact {
                        name: name.to_owned(),
                        kind: if active == GoBlock::Constants {
                            NodeKind::Constant
                        } else {
                            NodeKind::Custom("variable".to_owned())
                        },
                        span: span.clone(),
                    });
                }
            }
        }
        return true;
    }
    *block = match line {
        "import (" => Some(GoBlock::Imports),
        "const (" => Some(GoBlock::Constants),
        "var (" => Some(GoBlock::Variables),
        _ => None,
    };
    block.is_some()
}

fn quoted_segment(line: &str) -> Option<String> {
    let start = line.find(['"', '\'', '`'])?;
    let quote = line.as_bytes()[start] as char;
    let rest = &line[start + 1..];
    let end = rest.find(quote)?;
    let target = &rest[..end];
    (!target.is_empty()).then(|| target.to_owned())
}

/// `require('x')` and dynamic `import('x')` targets anywhere in the line.
fn script_call_imports(line: &str) -> Vec<String> {
    let mut targets = Vec::new();
    for marker in ["require(", "import("] {
        let mut rest = line;
        while let Some(position) = rest.find(marker) {
            let after = &rest[position + marker.len()..];
            if let Some(target) = quoted_segment(after.split(')').next().unwrap_or(after)) {
                targets.push(target);
            }
            rest = after;
        }
    }
    targets
}

/// Declarations that only exist at statement level inside an enclosing
/// function are execution details, not repository symbols.
fn scoped_out(kind: &NodeKind, language: &Language, owners: &[OwnerScope]) -> bool {
    let inside_callable = owners
        .last()
        .is_some_and(|scope| matches!(scope.locator.kind, NodeKind::Function | NodeKind::Method));
    if !inside_callable {
        return false;
    }
    match kind {
        NodeKind::Custom(name) if name == "field" => true,
        NodeKind::Constant | NodeKind::Custom(_) if *language == Language::Go => true,
        _ => false,
    }
}

#[derive(Debug, Clone, Copy)]
enum ScopeBoundary {
    Brace(i32),
    Indent(usize),
}

fn owns_scope(kind: &NodeKind, line: &str, language: &Language) -> bool {
    let owns = matches!(
        kind,
        NodeKind::Function | NodeKind::Method | NodeKind::Struct | NodeKind::Trait
    );
    owns && (*language == Language::Python || line.contains('{'))
}

fn scope_boundary(language: &Language, raw: &str, depth: i32) -> ScopeBoundary {
    if *language == Language::Python {
        ScopeBoundary::Indent(indentation(raw))
    } else {
        ScopeBoundary::Brace(depth.saturating_add(1))
    }
}

fn discard_completed_scopes(
    owners: &mut Vec<OwnerScope>,
    language: &Language,
    raw: &str,
    depth: i32,
) {
    let indentation = indentation(raw);
    while owners.last().is_some_and(|scope| match scope.boundary {
        ScopeBoundary::Brace(boundary) => depth < boundary,
        ScopeBoundary::Indent(boundary) => *language == Language::Python && indentation <= boundary,
    }) {
        owners.pop();
    }
}

fn indentation(raw: &str) -> usize {
    raw.chars()
        .take_while(|character| character.is_whitespace())
        .count()
}

fn import_targets(line: &str, language: &Language) -> Vec<String> {
    if *language == Language::Python {
        if let Some(value) = line.strip_prefix("from ") {
            return value
                .split_once(" import ")
                .map(|(module, _)| vec![module.to_owned()])
                .unwrap_or_default();
        }
        return line
            .strip_prefix("import ")
            .map(|value| {
                value
                    .split(',')
                    .filter_map(|item| item.split_whitespace().next())
                    .map(str::to_owned)
                    .collect()
            })
            .unwrap_or_default();
    }
    if matches!(language, Language::JavaScript | Language::TypeScript) {
        let mut targets = script_call_imports(line);
        // Closing line of a multi-line `import { ... } from 'x'`.
        if line.starts_with('}')
            && let Some((_, from)) = line.split_once(" from ")
        {
            if let Some(target) = quoted_segment(from) {
                targets.push(target);
            }
        } else if let Some(value) = line.strip_prefix("import ")
            && let Some(target) = quoted_segment(value)
        {
            targets.push(target);
        }
        targets.retain(|target| !target.is_empty());
        targets.dedup();
        return targets;
    }
    let value = match language {
        Language::Go => line
            .strip_prefix("import ")
            .or_else(|| line.starts_with('"').then_some(line)),
        Language::Java => line.strip_prefix("import "),
        Language::CSharp => line.strip_prefix("using "),
        Language::C | Language::Cpp => line.strip_prefix("#include "),
        Language::Bash => line
            .strip_prefix("source ")
            .or_else(|| line.strip_prefix(". ")),
        _ => None,
    };
    let Some(value) = value else {
        return Vec::new();
    };
    let value = if *language == Language::Java {
        value.trim_start().strip_prefix("static ").unwrap_or(value)
    } else {
        value
    };
    let target = value
        .rsplit_once(" from ")
        .map_or(value, |(_, from)| from)
        .trim()
        .trim_end_matches(';')
        .trim_matches(|character| matches!(character, '"' | '\'' | '<' | '>'));
    if target.is_empty() {
        Vec::new()
    } else {
        vec![target.to_owned()]
    }
}

fn call_names(line: &str) -> Vec<String> {
    let mut calls = Vec::new();
    for (index, byte) in line.as_bytes().iter().enumerate() {
        if *byte != b'(' {
            continue;
        }
        let name = line[..index]
            .trim_end()
            .rsplit(|character: char| !is_ident(character))
            .find(|part| !part.is_empty())
            .unwrap_or_default();
        if !name.is_empty() && !control_word(name) {
            calls.push(name.to_owned());
        }
    }
    calls.sort();
    calls.dedup();
    calls
}
