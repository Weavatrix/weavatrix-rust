use crate::language::Language;
use std::path::Path;

/// Rewrites a configured path into a repository-relative prefix.
pub(super) fn normalize_relative(value: &str) -> String {
    let value = value
        .trim_start_matches("./")
        .trim_end_matches('*')
        .trim_end_matches('/');
    if value == "." {
        String::new()
    } else {
        value.to_owned()
    }
}

pub(super) fn join_relative(prefix: &str, rest: &str) -> String {
    let rest = rest.trim_start_matches('/');
    match (prefix.is_empty(), rest.is_empty()) {
        (true, _) => rest.to_owned(),
        (false, true) => prefix.to_owned(),
        (false, false) => format!("{prefix}/{rest}"),
    }
}

pub(super) fn has_extension(path: &str, extension: &str) -> bool {
    Path::new(path)
        .extension()
        .is_some_and(|value| value.eq_ignore_ascii_case(extension))
}

pub(super) fn clean_specifier(value: &str) -> String {
    let token = value.split_whitespace().next().unwrap_or(value);
    let token = token.trim_matches(|character| matches!(character, '"' | '\'' | '<' | '>'));
    // A URL query or fragment is not part of the module path, but a leading
    // `#` is: that is how a package declares a subpath import.
    let (prefix, rest) = token.split_at(usize::from(token.starts_with('#')));
    let rest = rest.split(['?', '#']).next().unwrap_or(rest);
    format!("{prefix}{rest}")
}

pub(super) fn package_name(language: &Language, value: &str) -> String {
    let target = clean_specifier(value);
    match language {
        Language::Rust => target.split("::").next().unwrap_or(&target).to_owned(),
        Language::Python => target.split('.').next().unwrap_or(&target).to_owned(),
        Language::JavaScript | Language::TypeScript if target.starts_with('@') => {
            target.split('/').take(2).collect::<Vec<_>>().join("/")
        }
        Language::JavaScript | Language::TypeScript => {
            target.split('/').next().unwrap_or(&target).to_owned()
        }
        _ => target,
    }
}
