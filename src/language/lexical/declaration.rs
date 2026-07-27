use super::support::{control_word, identifier, is_ident, word_suffix};
use crate::language::Language;
use weavatrix_graph::NodeKind;

pub(super) fn declaration(line: &str, language: &Language) -> Option<(String, NodeKind)> {
    match language {
        Language::Go => go_declaration(line),
        Language::Python => python_declaration(line),
        Language::JavaScript | Language::TypeScript => script_declaration(line),
        Language::Java | Language::CSharp => object_declaration(line, language),
        Language::Bash => bash_declaration(line),
        Language::C | Language::Cpp => c_like_function(line, language),
        _ => None,
    }
}

pub(super) fn inheritance(
    line: &str,
    language: &Language,
) -> Vec<(String, weavatrix_graph::EdgeKind)> {
    if *language == Language::Python {
        return python_inheritance(line);
    }
    if !matches!(
        language,
        Language::JavaScript
            | Language::TypeScript
            | Language::Java
            | Language::CSharp
            | Language::Cpp
    ) {
        return Vec::new();
    }
    let mut result = Vec::new();
    for (keyword, relation) in [
        (" extends ", weavatrix_graph::EdgeKind::Inherits),
        (" implements ", weavatrix_graph::EdgeKind::Implements),
        (" : ", weavatrix_graph::EdgeKind::Inherits),
    ] {
        let Some((_, targets)) = line.split_once(keyword) else {
            continue;
        };
        for target in targets.split(',') {
            let name = identifier(target);
            if !name.is_empty() && !control_word(name) {
                result.push((name.to_owned(), relation.clone()));
            }
        }
    }
    result
}

fn python_inheritance(line: &str) -> Vec<(String, weavatrix_graph::EdgeKind)> {
    let Some(rest) = line.strip_prefix("class ") else {
        return Vec::new();
    };
    let Some(start) = rest.find('(') else {
        return Vec::new();
    };
    let bases = rest[start + 1..].split(')').next().unwrap_or_default();
    bases
        .split(',')
        .filter(|base| !base.contains('='))
        .map(|base| {
            let base = base.trim().trim_start_matches('*');
            identifier(base.rsplit('.').next().unwrap_or(base))
        })
        .filter(|name| !name.is_empty() && *name != "object" && !control_word(name))
        .map(|name| (name.to_owned(), weavatrix_graph::EdgeKind::Inherits))
        .collect()
}

fn go_declaration(line: &str) -> Option<(String, NodeKind)> {
    if let Some(mut rest) = line.strip_prefix("func ") {
        let kind = if rest.trim_start().starts_with('(') {
            let (_, after) = rest.split_once(')')?;
            rest = after;
            NodeKind::Method
        } else {
            NodeKind::Function
        };
        return named(rest, kind);
    }
    if let Some(rest) = line.strip_prefix("type ") {
        let name = identifier(rest);
        let kind = if rest.contains(" interface") {
            NodeKind::Trait
        } else if rest.contains(" struct") {
            NodeKind::Struct
        } else {
            NodeKind::TypeAlias
        };
        return (!name.is_empty()).then(|| (name.to_owned(), kind));
    }
    line.strip_prefix("const ")
        .and_then(|rest| named(rest, NodeKind::Constant))
        .or_else(|| {
            line.strip_prefix("var ")
                .and_then(|rest| named(rest, variable()))
        })
}

fn python_declaration(line: &str) -> Option<(String, NodeKind)> {
    line.strip_prefix("async def ")
        .or_else(|| line.strip_prefix("def "))
        .and_then(|rest| named(rest, NodeKind::Function))
        .or_else(|| {
            line.strip_prefix("class ")
                .and_then(|rest| named(rest, NodeKind::Struct))
        })
}

fn script_declaration(line: &str) -> Option<(String, NodeKind)> {
    let line = strip_modifiers(line, &["export", "default", "declare"]);
    for (prefix, kind) in [
        ("async function ", NodeKind::Function),
        ("function ", NodeKind::Function),
        ("class ", NodeKind::Struct),
        ("interface ", NodeKind::Trait),
        ("enum ", NodeKind::Enum),
        ("type ", NodeKind::TypeAlias),
    ] {
        if let Some(rest) = line.strip_prefix(prefix) {
            return named(rest, kind);
        }
    }
    for prefix in ["const ", "let ", "var "] {
        if let Some(rest) = line.strip_prefix(prefix) {
            let kind = if rest.contains("=>") {
                NodeKind::Function
            } else {
                variable()
            };
            return named(rest, kind);
        }
    }
    None
}

fn object_declaration(line: &str, language: &Language) -> Option<(String, NodeKind)> {
    for (prefix, kind) in [
        ("class ", NodeKind::Struct),
        ("interface ", NodeKind::Trait),
        ("enum ", NodeKind::Enum),
        ("record ", NodeKind::Struct),
    ] {
        if let Some(rest) = line
            .strip_prefix(prefix)
            .or_else(|| word_suffix(line, prefix))
        {
            return named(rest, kind);
        }
    }
    c_like_function(line, language).or_else(|| object_field(line))
}

/// A `Type name;` / `Type name = ...;` member declaration. Statement-level
/// false positives inside method bodies are filtered by scope in the parser.
fn object_field(line: &str) -> Option<(String, NodeKind)> {
    if !line.ends_with(';') {
        return None;
    }
    let head = line.split(['=', ';']).next()?.trim_end();
    if head.contains('(') || head.contains(['.', '"', '+', '-', '/', '!']) {
        return None;
    }
    let head = strip_modifiers(
        head,
        &[
            "public",
            "private",
            "protected",
            "static",
            "final",
            "volatile",
            "transient",
            "readonly",
            "const",
        ],
    );
    if head.starts_with('@') {
        return None;
    }
    let name = head
        .rsplit(|character: char| !is_ident(character))
        .find(|part| !part.is_empty())?;
    let declared_type = head[..head.len() - name.len()].trim_end();
    let has_type = declared_type
        .chars()
        .next()
        .is_some_and(|character| character.is_alphabetic() || character == '_');
    (has_type && !control_word(name) && !control_word(declared_type))
        .then(|| (name.to_owned(), NodeKind::Custom("field".to_owned())))
}

fn bash_declaration(line: &str) -> Option<(String, NodeKind)> {
    line.strip_prefix("function ")
        .and_then(|rest| named(rest, NodeKind::Function))
        .or_else(|| (line.contains("()")).then(|| named(line, NodeKind::Function))?)
}

fn c_like_function(line: &str, language: &Language) -> Option<(String, NodeKind)> {
    if !line.contains('(') || !(line.ends_with('{') || line.ends_with(") {")) {
        return None;
    }
    let before = line.split_once('(')?.0.trim_end();
    let name = before
        .rsplit(|character: char| !is_ident(character))
        .find(|part| !part.is_empty())?;
    // `.name(` is a call chain and `@Name(` is an annotation, never the
    // declared function itself.
    let preceding = before[..before.len() - name.len()].chars().next_back();
    if matches!(preceding, Some('.' | '@')) {
        return None;
    }
    let kind = if matches!(language, Language::Java | Language::CSharp) {
        NodeKind::Method
    } else {
        NodeKind::Function
    };
    (!control_word(name)).then(|| (name.to_owned(), kind))
}

fn named(value: &str, kind: NodeKind) -> Option<(String, NodeKind)> {
    let name = identifier(value);
    (!name.is_empty()).then(|| (name.to_owned(), kind))
}

fn strip_modifiers<'line>(mut line: &'line str, modifiers: &[&str]) -> &'line str {
    loop {
        let Some((word, rest)) = line.split_once(char::is_whitespace) else {
            return line;
        };
        if !modifiers.contains(&word) {
            return line;
        }
        line = rest.trim_start();
    }
}

fn variable() -> NodeKind {
    NodeKind::Custom("variable".to_owned())
}

#[cfg(test)]
mod tests {
    use super::declaration;
    use crate::language::Language;
    use weavatrix_graph::NodeKind;

    #[test]
    fn classifies_script_declarations_without_treating_jsx_as_methods() {
        assert_eq!(
            declaration("const load = async () => {", &Language::TypeScript),
            Some(("load".to_owned(), NodeKind::Function))
        );
        assert_eq!(
            declaration("const value = useValue();", &Language::TypeScript),
            Some(("value".to_owned(), NodeKind::Custom("variable".to_owned())))
        );
        assert_eq!(declaration("{isReady && (", &Language::TypeScript), None);
        assert_eq!(declaration("request({", &Language::JavaScript), None);
    }

    #[test]
    fn distinguishes_go_methods_structs_and_interfaces() {
        assert_eq!(
            declaration("func (s *Server) Start() {", &Language::Go),
            Some(("Start".to_owned(), NodeKind::Method))
        );
        assert_eq!(
            declaration("type Config struct {", &Language::Go),
            Some(("Config".to_owned(), NodeKind::Struct))
        );
        assert_eq!(
            declaration("type Runner interface {", &Language::Go),
            Some(("Runner".to_owned(), NodeKind::Trait))
        );
    }
}
