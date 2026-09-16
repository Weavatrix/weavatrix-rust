//! Mermaid flowchart evidence. Drawn arrows stay `declared_architecture`.

mod adapter;
mod bind;
mod detect;
mod edges;
mod facts;
mod flowchart;
mod lex;
mod model;
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
