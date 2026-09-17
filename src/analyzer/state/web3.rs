use super::AnalysisState;
use crate::language::web3::web3_binds;
use crate::model::Result;
use std::collections::BTreeSet;
use weavatrix_graph::{Edge, EdgeKind, NodeId, NodeKind, SourceSpan};

impl AnalysisState {
    pub(super) fn register_web3_symbol(
        &mut self,
        relative: &str,
        kind: &NodeKind,
        name: &str,
        id: &NodeId,
    ) {
        let kind = kind.as_str();
        if !kind.starts_with("web3.abi.") {
            return;
        }
        let short = kind.trim_start_matches("web3.abi.");
        let stem = name.split('(').next().unwrap_or(name);
        for key in [
            format!("{relative}#{short}:{name}"),
            format!("{relative}#{short}:{stem}"),
            format!("{relative}#abi"),
        ] {
            self.web3_index.entry(key).or_default().push(id.clone());
        }
    }

    pub(super) fn queue_web3_bind(
        &mut self,
        source: &NodeId,
        name: &str,
        span: SourceSpan,
        relation: EdgeKind,
    ) {
        let Some(key) = name.strip_prefix("web3.bind:") else {
            return;
        };
        self.pending_web3
            .push((source.clone(), key.to_owned(), span, relation));
    }

    pub(in crate::analyzer) fn resolve_web3_bindings(&mut self) -> Result<()> {
        let pending = std::mem::take(&mut self.pending_web3);
        let mut seen = BTreeSet::<(String, String, String, String)>::new();
        for (from, key, span, _relation) in pending {
            for to in self.lookup_web3(&key) {
                let detail = format!("web3-bind:{key}");
                if !seen.insert((
                    from.as_str().to_owned(),
                    to.as_str().to_owned(),
                    "web3.binds".into(),
                    detail.clone(),
                )) {
                    continue;
                }
                let provenance = crate::analyzer::support::parsed_provenance(
                    "weavatrix.web3",
                    Some(span.clone()),
                )?
                .with_detail(detail);
                self.graph
                    .add_edge(Edge::new(from.clone(), to, web3_binds(), provenance))?;
            }
        }
        Ok(())
    }

    fn lookup_web3(&self, key: &str) -> Vec<NodeId> {
        if let Some(exact) = self.web3_index.get(key) {
            return exact.clone();
        }
        self.web3_index
            .iter()
            .filter(|(candidate, _)| candidate.starts_with(key))
            .flat_map(|(_, ids)| ids.clone())
            .collect()
    }
}
