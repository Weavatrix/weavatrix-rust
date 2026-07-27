mod declaration;
mod domain;
mod support;

use super::{
    FileFacts, ImportFact, Language, LanguageAdapter, ReferenceFact, SourceFile, SymbolFact,
    SymbolLocator,
};
use crate::Result;
use declaration::{declaration, inheritance};
use domain::{
    domain_facts, object_route_key, object_route_method, parse_sql, parse_yaml, route_fact,
};
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
    for (offset, raw) in source.text.lines().enumerate() {
        let line_number = line_number(offset);
        let line = raw.trim();
        if line.is_empty() || line.starts_with("//") || line.starts_with('#') {
            continue;
        }
        discard_completed_scopes(&mut owners, language, raw, depth);
        let span = line_span(source.path, line_number, raw);
        let declaration = declaration(line, language);
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
        if route_objects {
            if let Some((path, block)) = object_route_key(line) {
                if block {
                    object_route = Some(path);
                } else {
                    facts
                        .domains
                        .push(route_fact("ANY", &path, &span, owner.clone()));
                    object_route = None;
                }
            } else if let Some(method) = object_route_method(line)
                && let Some(path) = &object_route
            {
                facts
                    .domains
                    .push(route_fact(method, path, &span, owner.clone()));
            } else if line.starts_with('}') {
                object_route = None;
            }
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
        depth += brace_delta(line);
    }
    sort_facts(&mut facts);
    facts
}

#[derive(Debug, Clone)]
struct OwnerScope {
    locator: SymbolLocator,
    boundary: ScopeBoundary,
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
    let value = match language {
        Language::Go => line
            .strip_prefix("import ")
            .or_else(|| line.starts_with('"').then_some(line)),
        Language::Java | Language::JavaScript | Language::TypeScript => {
            line.strip_prefix("import ")
        }
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
