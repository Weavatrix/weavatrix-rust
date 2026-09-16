use super::detect::normalized;
use crate::language::yaml_doc;
use weavatrix_graph::{SourcePosition, SourceSpan};

#[must_use]
pub(super) fn package_root(path: &str) -> String {
    let normalized = normalized(path);
    for marker in [
        "/.cursor-plugin/",
        "/.claude-plugin/",
        "/.codex-plugin/",
        "/.grok-plugin/",
        "/dev.kiro/",
        "/skills/",
    ] {
        if let Some(index) = normalized.find(marker) {
            return normalized[..index].to_owned();
        }
    }
    dirname(&normalized)
}

#[must_use]
pub(super) fn dirname(path: &str) -> String {
    let normalized = normalized(path);
    normalized
        .rsplit_once('/')
        .map_or_else(String::new, |(parent, _)| parent.to_owned())
}

#[must_use]
pub(super) fn parent_dir_name(path: &str) -> String {
    dirname(path)
        .rsplit('/')
        .next()
        .unwrap_or_default()
        .to_owned()
}

/// Textual containment only. The analyzer does not follow symlinks or launch paths.
#[must_use]
pub(super) fn plugin_relative(value: &str) -> bool {
    value.starts_with("./") && !escapes_root(value)
}

#[must_use]
pub(super) fn escapes_root(value: &str) -> bool {
    let trimmed = value.replace('\\', "/");
    trimmed.split('/').any(|part| part == "..")
        || trimmed.starts_with('/')
        || looks_absolute_windows(&trimmed)
}

fn looks_absolute_windows(value: &str) -> bool {
    let mut chars = value.chars();
    matches!(chars.next(), Some(letter) if letter.is_ascii_alphabetic())
        && chars.next() == Some(':')
}

#[must_use]
pub(super) fn file_span(path: &str) -> SourceSpan {
    SourceSpan::new(path, SourcePosition::new(1, 1), SourcePosition::new(1, 2))
}

#[must_use]
pub(super) fn span_of(path: &str, raw: &str, needle: &str) -> SourceSpan {
    raw.find(needle).map_or_else(
        || file_span(path),
        |start| yaml_doc::span_for(path, raw, start, start + needle.len()),
    )
}

#[cfg(test)]
mod tests {
    use super::{escapes_root, package_root, plugin_relative};

    #[test]
    fn package_root_stops_at_client_overlay_and_skills() {
        assert_eq!(
            package_root("plugins/demo/.cursor-plugin/plugin.json"),
            "plugins/demo"
        );
        assert_eq!(
            package_root("plugins/demo/skills/lookup/SKILL.md"),
            "plugins/demo"
        );
        assert!(plugin_relative("./bin/server"));
        assert!(escapes_root("../bin/server"));
        assert!(!plugin_relative("data"));
    }
}
