use super::AnalysisState;
use crate::analyzer::support::{locator_key, parsed_provenance, sanitize_id};
use crate::language::{BoundEdgeFact, DomainFact};
use crate::model::Result;
use std::collections::{BTreeMap, BTreeSet};
use weavatrix_graph::{Edge, Node, NodeId, NodeKind};

impl AnalysisState {
    pub(super) fn add_domains(
        &mut self,
        file_id: &NodeId,
        extractor: &'static str,
        domains: Vec<DomainFact>,
        local_symbols: &BTreeMap<(NodeKind, String, u32, u32), NodeId>,
    ) -> Result<()> {
        for fact in domains {
            let source = fact
                .owner
                .as_ref()
                .and_then(|owner| {
                    local_symbols
                        .get(&locator_key(&owner.kind, &owner.name, &owner.span))
                        .cloned()
                })
                .unwrap_or_else(|| file_id.clone());
            let (id, created) = self.domain_id(&fact.kind, &fact.name, &source)?;
            if !created {
                continue;
            }
            self.graph.add_node(
                Node::new(id.to_string(), fact.name, fact.kind)?.with_span(fact.span.clone()),
            )?;
            let provenance = parsed_provenance(extractor, Some(fact.span))?
                .with_detail("domain evidence extracted from source");
            self.graph
                .add_edge(Edge::new(source, id, fact.relation, provenance))?;
        }
        Ok(())
    }

    /// Two labels can sanitize to one identifier: `ANY /$` and `ANY /:` are
    /// both `ANY___`. The graph refuses to merge nodes that differ only in
    /// label, and one such pair must not abort the whole analysis, so the
    /// later label takes a numbered identifier instead.
    pub(super) fn add_bound_edges(
        &mut self,
        extractor: &'static str,
        edges: Vec<BoundEdgeFact>,
        local_symbols: &BTreeMap<(NodeKind, String, u32, u32), NodeId>,
    ) -> Result<()> {
        let mut seen = BTreeSet::<(NodeId, NodeId, String)>::new();
        for fact in edges {
            let Some(from) = local_symbols.get(&locator_key(
                &fact.from.kind,
                &fact.from.name,
                &fact.from.span,
            )) else {
                continue;
            };
            let Some(to) =
                local_symbols.get(&locator_key(&fact.to.kind, &fact.to.name, &fact.to.span))
            else {
                continue;
            };
            if !seen.insert((from.clone(), to.clone(), fact.kind.as_str().to_owned())) {
                continue;
            }
            let detail = if fact.detail.is_empty() {
                "bound domain edge with exact endpoints".to_owned()
            } else {
                fact.detail
            };
            let provenance = parsed_provenance(extractor, Some(fact.span))?.with_detail(detail);
            self.graph
                .add_edge(Edge::new(from.clone(), to.clone(), fact.kind, provenance))?;
        }
        Ok(())
    }

    fn domain_id(
        &mut self,
        kind: &NodeKind,
        label: &str,
        owner: &NodeId,
    ) -> Result<(NodeId, bool)> {
        let base = format!(
            "domain:{}:{}@{}",
            kind.as_str(),
            sanitize_id(label),
            sanitize_id(owner.as_str())
        );
        let mut candidate = base.clone();
        let mut ordinal = 1_u32;
        loop {
            let id = NodeId::new(candidate)?;
            match self.domain_labels.get(&id) {
                Some(existing) if existing != label => {
                    ordinal += 1;
                    candidate = format!("{base}~{ordinal}");
                }
                Some(_) => return Ok((id, false)),
                None => {
                    self.domain_labels.insert(id.clone(), label.to_owned());
                    return Ok((id, true));
                }
            }
        }
    }
}
