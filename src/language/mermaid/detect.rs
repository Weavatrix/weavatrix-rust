use std::path::Path;

#[must_use]
pub(crate) fn looks_promising(path: &str, raw: &str) -> bool {
    is_standalone(path) || has_mermaid_fence(raw) || has_flowchart_header(raw)
}

#[must_use]
pub(super) fn is_standalone(path: &str) -> bool {
    matches!(
        Path::new(path)
            .extension()
            .and_then(|value| value.to_str())
            .map(str::to_ascii_lowercase)
            .as_deref(),
        Some("mmd" | "mermaid")
    )
}

#[must_use]
pub(super) fn is_links_file(path: &str) -> bool {
    Path::new(path)
        .file_name()
        .and_then(|value| value.to_str())
        .is_some_and(|name| name.eq_ignore_ascii_case("diagram-links.json"))
}

fn has_mermaid_fence(raw: &str) -> bool {
    raw.contains("```mermaid")
        || raw.contains("~~~mermaid")
        || raw.contains("```Mermaid")
        || raw.contains("~~~Mermaid")
}

fn has_flowchart_header(raw: &str) -> bool {
    raw.lines()
        .map(str::trim_start)
        .filter(|line| !line.is_empty() && !line.starts_with("%%"))
        .take(3)
        .any(|line| line.starts_with("flowchart") || line.starts_with("graph ") || line == "graph")
}

#[cfg(test)]
mod tests {
    use super::{is_links_file, is_standalone, looks_promising};

    #[test]
    fn mermaid_probes_cover_extension_fence_and_header() {
        assert!(is_standalone("flow.mmd"));
        assert!(is_standalone("Flow.MERMAID"));
        assert!(!is_standalone("flow.md"));
        assert!(is_links_file("diagram-links.json"));
        assert!(!is_links_file("other.json"));
        assert!(looks_promising(
            "doc.md",
            "```mermaid\nflowchart TD\nA-->B\n```"
        ));
        assert!(looks_promising("doc.md", "~~~Mermaid\nflowchart LR\n```"));
        assert!(looks_promising(
            "flow.txt",
            "%% comment\n\ngraph TD\nA-->B\n"
        ));
        assert!(looks_promising("flow.txt", "graph\nA-->B\n"));
        assert!(!looks_promising("note.md", "just a sentence"));
    }
}
