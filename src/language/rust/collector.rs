use super::{
    Collector, DomainFact, OwnerUpdate, ReferenceFact, SymbolFact, SymbolLocator, attribute_routes,
    source_span,
};
use proc_macro2::Span;
use weavatrix_graph::{EdgeKind, NodeKind};

impl Collector<'_> {
    pub(super) fn add_symbol(
        &mut self,
        name: &syn::Ident,
        kind: NodeKind,
        definition_span: Span,
        exported: bool,
    ) -> SymbolLocator {
        let mut span = source_span(self.path, name.span());
        span.end = source_span(self.path, definition_span).end;
        let locator = SymbolLocator {
            name: name.to_string(),
            kind,
            span,
        };
        self.facts.symbols.push(SymbolFact {
            name: locator.name.clone(),
            kind: locator.kind.clone(),
            span: locator.span.clone(),
            test_only: self.test_context,
            exported,
            source_fingerprint: None,
            source_extent: None,
            owner: (locator.kind == NodeKind::Method)
                .then(|| self.owner.type_name.clone())
                .flatten(),
        });
        locator
    }

    pub(super) fn with_owner(&mut self, update: OwnerUpdate, visit: impl FnOnce(&mut Self)) {
        let previous = self.owner.apply(update);
        visit(self);
        self.owner = previous;
    }

    pub(super) fn with_test_context(
        &mut self,
        attributes: &[syn::Attribute],
        visit: impl FnOnce(&mut Self),
    ) {
        let previous = self.test_context;
        self.test_context |= super::attributes_mark_test(attributes);
        visit(self);
        self.test_context = previous;
    }

    pub(super) fn add_reference(
        &mut self,
        name: String,
        kind: EdgeKind,
        qualified: bool,
        span: Span,
    ) {
        self.facts.references.push(ReferenceFact {
            name,
            kind,
            receiver: None,
            qualified,
            span: source_span(self.path, span),
            owner: self.owner.symbol.clone(),
        });
    }

    pub(super) fn add_endpoint(&mut self, method: &str, path: &str, span: Span) {
        self.facts.domains.push(DomainFact {
            name: format!("{method} {path}"),
            kind: NodeKind::Endpoint,
            relation: EdgeKind::Exposes,
            span: source_span(self.path, span),
            owner: self.owner.symbol.clone(),
        });
    }

    pub(super) fn add_attribute_endpoints(&mut self, attributes: &[syn::Attribute]) {
        for (method, path, span) in attribute_routes(attributes) {
            self.add_endpoint(method, &path, span);
        }
    }
}
