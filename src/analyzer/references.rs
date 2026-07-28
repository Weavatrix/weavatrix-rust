use crate::Result;
use crate::language::{Language, ReferenceFact};
use std::collections::HashMap;
use weavatrix_graph::{Confidence, Edge, EvidenceKind, GraphBuilder, NodeId, Provenance};

pub(super) struct PendingReference {
    pub source: NodeId,
    pub language: Language,
    pub extractor: &'static str,
    pub reference: ReferenceFact,
}

pub(super) fn resolve(
    graph: &mut GraphBuilder,
    symbols: &HashMap<Language, HashMap<String, Vec<NodeId>>>,
    references: Vec<PendingReference>,
) -> Result<()> {
    for item in references {
        let Some(targets) = symbols
            .get(&item.language)
            .and_then(|names| names.get(item.reference.name.as_str()))
        else {
            continue;
        };
        if targets.len() != 1 || targets[0] == item.source {
            continue;
        }
        let provenance = Provenance::new(item.extractor, EvidenceKind::Resolved, Confidence::High)?
            .with_span(item.reference.span)
            .with_detail("unique repository symbol match");
        graph.add_edge(Edge::new(
            item.source,
            targets[0].clone(),
            item.reference.kind,
            provenance,
        ))?;
    }
    Ok(())
}
