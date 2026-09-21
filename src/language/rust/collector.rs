use super::{
    Collector, DomainFact, OwnerUpdate, ReferenceFact, SymbolFact, SymbolLocator,
    associated_owner_name, attribute_routes, bare_path_name, call_is_locally_scoped, callable_name,
    route_call, source_span,
};
use proc_macro2::Span;
use syn::spanned::Spanned;
use weavatrix_graph::{EdgeKind, NodeKind};

impl Collector<'_> {
    pub(super) fn collect_trait_implementation(&mut self, node: &syn::ItemImpl) {
        if let Some((trait_path, _)) = &node.trait_
            && let Some(trait_name) = trait_path.segments.last()
        {
            self.add_reference(
                trait_name.ident.to_string(),
                EdgeKind::Implements,
                false,
                trait_name.ident.span(),
            );
        }
    }

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

    /// The call itself, the bare-path arguments it names, and any route it
    /// registers.
    pub(super) fn collect_method_call(&mut self, node: &syn::ExprMethodCall) {
        self.add_qualified_call(
            node.method.to_string(),
            bare_path_name(&node.receiver),
            node.span(),
        );
        for argument in &node.args {
            if let Some(name) = bare_path_name(argument) {
                self.add_reference(name, EdgeKind::References, false, argument.span());
            }
        }
        if node.method == "route" {
            for (method, path) in route_call(node) {
                self.add_endpoint(method, &path, node.span());
            }
        }
    }

    /// A call through a path or a field expression.
    ///
    /// A bare name, or a `self`, `super` or `crate` path, is written in this
    /// file's own scope. `Repository::open(..)` ends in somebody else's path,
    /// and binding it by that final segment alone is how one unrelated `open`
    /// collects every `File::open` in a repository - the same mistake
    /// `visit_type_path` already refuses to make for types. The owning segment
    /// of a two-segment path is still recorded as a reference, so the coupling
    /// survives without the invented call.
    pub(super) fn collect_call(&mut self, node: &syn::ExprCall) {
        if let Some(name) = callable_name(&node.func) {
            if call_is_locally_scoped(&node.func) {
                self.add_reference(name, EdgeKind::Calls, false, node.span());
            } else {
                self.add_qualified_call(name, None, node.span());
            }
        }
        if let Some(name) = associated_owner_name(&node.func) {
            self.add_reference(name, EdgeKind::References, false, node.func.span());
        }
    }

    /// Records a call whose target lives in a namespace this file does not
    /// own: a method on the receiver's type, or the tail of somebody else's
    /// path. It is marked qualified and carries the receiver it was written
    /// on, which is what keeps `values.push(item)` away from an unrelated
    /// `fn push`.
    pub(super) fn add_qualified_call(
        &mut self,
        name: String,
        receiver: Option<String>,
        span: Span,
    ) {
        self.facts.references.push(ReferenceFact {
            name,
            kind: EdgeKind::Calls,
            receiver,
            qualified: true,
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
