//! Mermaid flowchart evidence. Drawn arrows stay `declared_architecture`.

mod adapter;
#[path = "flow/bind.rs"]
mod bind;
mod detect;
#[path = "flow/edges.rs"]
mod edges;
mod facts;
#[path = "flow/flowchart.rs"]
mod flowchart;
#[path = "flow/lex.rs"]
mod lex;
mod model;
#[path = "flow/regions.rs"]
mod regions;
mod span;

pub(crate) use adapter::MermaidAdapter;
pub(crate) use bind::analyze_links;
pub(crate) use detect::looks_promising;

use crate::language::FileFacts;

#[must_use]
pub(crate) fn analyze_standalone(path: &str, raw: &str) -> FileFacts {
    facts::to_file_facts(path, &parse_regions(path, raw))
}

#[must_use]
pub(crate) fn analyze_markdown(path: &str, raw: &str) -> Option<FileFacts> {
    let diagrams = parse_regions(path, raw);
    if diagrams.is_empty() {
        None
    } else {
        Some(facts::to_file_facts(path, &diagrams))
    }
}

fn parse_regions(path: &str, raw: &str) -> Vec<model::Diagram> {
    regions::extract(path, raw)
        .into_iter()
        .map(|region| flowchart::parse(path, raw, &region))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::{analyze_markdown, analyze_standalone, looks_promising};

    #[test]
    fn standalone_and_fenced_flowcharts_are_diagrams() {
        assert!(looks_promising("a.mmd", ""));
        let standalone = analyze_standalone("a.mmd", "flowchart TD\nA-->B\n");
        assert!(!standalone.symbols.is_empty());
        assert!(analyze_markdown("doc.md", "no diagram").is_none());
        let fenced = analyze_markdown("doc.md", "```mermaid\nflowchart LR\nA-->B\n```").unwrap();
        assert!(!fenced.symbols.is_empty());
        let rich = analyze_standalone(
            "rich.mmd",
            "%%{init: {'theme':'dark'}}%%\n\
flowchart TB\n\
subgraph nest [Nest]\n\
  A[Alpha] -->|go| B((Beta))\n\
end\n\
classDef dim fill:#eee\n\
style A fill:#f9f\n\
click A callback\n\
bad line without arrow\n",
        );
        assert!(
            rich.symbols
                .iter()
                .any(|symbol| symbol.name.contains("Nest") || symbol.name == "A")
        );
        let missing = analyze_standalone("empty.mmd", "not a diagram\n");
        assert!(
            missing
                .diagnostics
                .iter()
                .any(|item| item.code == "mermaid.unsupported")
        );
    }
}
