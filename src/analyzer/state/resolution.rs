use super::AnalysisState;
use crate::analyzer::imports::resolve as resolve_imports;
use crate::analyzer::references::resolve as resolve_references;
use crate::analyzer::support::parsed_provenance;
use crate::model::Result;
use weavatrix_graph::{Edge, EdgeKind};

impl AnalysisState {
    pub(in crate::analyzer) fn resolve_references(&mut self) -> Result<()> {
        let (scopes, unresolved) = resolve_imports(
            &mut self.graph,
            &self.file_index,
            &self.repository_label,
            &self.root,
            std::mem::take(&mut self.pending_imports),
            std::mem::take(&mut self.pending_reexports),
        )?;
        self.diagnostics.extend(unresolved);
        // Every file has been read, so a type declared in one file can claim
        // members another file wrote for it. Ambiguity remains unresolved.
        for (language, owner, member, span, extractor) in std::mem::take(&mut self.pending_methods)
        {
            let Some(candidates) = self
                .symbol_index
                .get(&language)
                .and_then(|names| names.get(&owner))
            else {
                continue;
            };
            if let [only] = candidates.as_slice()
                && *only != member
            {
                self.graph.add_edge(Edge::new(
                    only.clone(),
                    member,
                    EdgeKind::Method,
                    parsed_provenance(extractor, Some(span))?,
                ))?;
            }
        }
        resolve_references(
            &mut self.graph,
            &self.symbol_index,
            &self.scoped_symbols,
            &scopes,
            std::mem::take(&mut self.pending_references),
        )
    }
}
