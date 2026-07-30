use super::{
    DomainFact, FileFacts, ImportFact, Language, LanguageAdapter, ReferenceFact, SourceFile,
    SymbolFact, SymbolLocator,
};
use crate::model::{Diagnostic, Result};
use proc_macro2::Span;
use syn::spanned::Spanned;
use syn::visit::Visit;
use weavatrix_graph::{EdgeKind, NodeKind};

use endpoints::{attribute_routes, callable_name, route_call};
use syntax::{attributes_mark_test, impl_owner, source_span, use_tree_targets};

mod endpoints;
mod syntax;

#[derive(Debug, Clone, Copy)]
pub struct RustAdapter;

impl LanguageAdapter for RustAdapter {
    fn language(&self) -> Language {
        Language::Rust
    }

    fn extensions(&self) -> &'static [&'static str] {
        &["rs"]
    }

    fn extractor(&self) -> &'static str {
        "weavatrix.rust.syn"
    }

    fn parse(&self, source: SourceFile<'_>) -> Result<FileFacts> {
        let syntax = match syn::parse_file(source.text) {
            Ok(syntax) => syntax,
            Err(error) => {
                return Ok(FileFacts {
                    diagnostics: vec![Diagnostic {
                        code: "rust.syntax_error".into(),
                        message: error.to_string(),
                        span: Some(source_span(source.path, error.span())),
                    }],
                    ..FileFacts::default()
                });
            }
        };

        let mut collector = Collector {
            path: source.path,
            facts: FileFacts::default(),
            owner: OwnerScope::default(),
            test_context: false,
        };
        collector.visit_file(&syntax);
        collector
            .facts
            .symbols
            .sort_by(|left, right| left.span.cmp(&right.span));
        collector
            .facts
            .references
            .sort_by(|left, right| left.span.cmp(&right.span));
        collector
            .facts
            .imports
            .sort_by(|left, right| left.span.cmp(&right.span));
        Ok(collector.facts)
    }
}

struct Collector<'source> {
    path: &'source str,
    facts: FileFacts,
    owner: OwnerScope,
    test_context: bool,
}

#[derive(Clone, Default)]
struct OwnerScope {
    symbol: Option<SymbolLocator>,
    type_name: Option<String>,
}

enum OwnerUpdate {
    Symbol(SymbolLocator),
    Type(String),
}

impl Collector<'_> {
    fn add_symbol(&mut self, name: &syn::Ident, kind: NodeKind, span: Span) -> SymbolLocator {
        let locator = SymbolLocator {
            name: name.to_string(),
            kind,
            span: source_span(self.path, span),
        };
        self.facts.symbols.push(SymbolFact {
            name: locator.name.clone(),
            kind: locator.kind.clone(),
            span: locator.span.clone(),
            test_only: self.test_context,
            owner: (locator.kind == NodeKind::Method)
                .then(|| self.owner.type_name.clone())
                .flatten(),
        });
        locator
    }

    fn with_owner(&mut self, update: OwnerUpdate, visit: impl FnOnce(&mut Self)) {
        let previous = self.owner.clone();
        match update {
            OwnerUpdate::Symbol(owner) => self.owner.symbol = Some(owner),
            OwnerUpdate::Type(owner) => self.owner.type_name = Some(owner),
        }
        visit(self);
        self.owner = previous;
    }

    fn with_test_context(&mut self, attributes: &[syn::Attribute], visit: impl FnOnce(&mut Self)) {
        let previous = self.test_context;
        self.test_context |= attributes_mark_test(attributes);
        visit(self);
        self.test_context = previous;
    }

    fn add_reference(&mut self, name: String, span: Span) {
        self.facts.references.push(ReferenceFact {
            name,
            kind: EdgeKind::Calls,
            receiver: None,
            qualified: false,
            span: source_span(self.path, span),
            owner: self.owner.symbol.clone(),
        });
    }

    fn add_endpoint(&mut self, method: &str, path: &str, span: Span) {
        self.facts.domains.push(DomainFact {
            name: format!("{method} {path}"),
            kind: NodeKind::Endpoint,
            relation: EdgeKind::Exposes,
            span: source_span(self.path, span),
            owner: self.owner.symbol.clone(),
        });
    }

    fn add_attribute_endpoints(&mut self, attributes: &[syn::Attribute]) {
        for (method, path, span) in attribute_routes(attributes) {
            self.add_endpoint(method, &path, span);
        }
    }
}

impl<'ast> Visit<'ast> for Collector<'_> {
    fn visit_item_fn(&mut self, node: &'ast syn::ItemFn) {
        self.with_test_context(&node.attrs, |collector| {
            let owner =
                collector.add_symbol(&node.sig.ident, NodeKind::Function, node.sig.ident.span());
            collector.with_owner(OwnerUpdate::Symbol(owner), |collector| {
                collector.add_attribute_endpoints(&node.attrs);
                syn::visit::visit_item_fn(collector, node);
            });
        });
    }

    fn visit_impl_item_fn(&mut self, node: &'ast syn::ImplItemFn) {
        self.with_test_context(&node.attrs, |collector| {
            let owner =
                collector.add_symbol(&node.sig.ident, NodeKind::Method, node.sig.ident.span());
            collector.with_owner(OwnerUpdate::Symbol(owner), |collector| {
                collector.add_attribute_endpoints(&node.attrs);
                syn::visit::visit_impl_item_fn(collector, node);
            });
        });
    }

    fn visit_trait_item_fn(&mut self, node: &'ast syn::TraitItemFn) {
        self.with_test_context(&node.attrs, |collector| {
            let owner =
                collector.add_symbol(&node.sig.ident, NodeKind::Method, node.sig.ident.span());
            collector.with_owner(OwnerUpdate::Symbol(owner), |collector| {
                syn::visit::visit_trait_item_fn(collector, node);
            });
        });
    }

    fn visit_item_struct(&mut self, node: &'ast syn::ItemStruct) {
        self.with_test_context(&node.attrs, |collector| {
            collector.add_symbol(&node.ident, NodeKind::Struct, node.ident.span());
            syn::visit::visit_item_struct(collector, node);
        });
    }

    fn visit_item_enum(&mut self, node: &'ast syn::ItemEnum) {
        self.with_test_context(&node.attrs, |collector| {
            collector.add_symbol(&node.ident, NodeKind::Enum, node.ident.span());
            syn::visit::visit_item_enum(collector, node);
        });
    }

    fn visit_item_trait(&mut self, node: &'ast syn::ItemTrait) {
        self.with_test_context(&node.attrs, |collector| {
            collector.add_symbol(&node.ident, NodeKind::Trait, node.ident.span());
            collector.with_owner(OwnerUpdate::Type(node.ident.to_string()), |collector| {
                syn::visit::visit_item_trait(collector, node);
            });
        });
    }

    fn visit_item_impl(&mut self, node: &'ast syn::ItemImpl) {
        self.with_test_context(&node.attrs, |collector| {
            if let Some(owner) = impl_owner(&node.self_ty) {
                collector.with_owner(OwnerUpdate::Type(owner), |collector| {
                    syn::visit::visit_item_impl(collector, node);
                });
            } else {
                syn::visit::visit_item_impl(collector, node);
            }
        });
    }

    fn visit_item_type(&mut self, node: &'ast syn::ItemType) {
        self.with_test_context(&node.attrs, |collector| {
            collector.add_symbol(&node.ident, NodeKind::TypeAlias, node.ident.span());
            syn::visit::visit_item_type(collector, node);
        });
    }

    fn visit_item_const(&mut self, node: &'ast syn::ItemConst) {
        self.with_test_context(&node.attrs, |collector| {
            collector.add_symbol(&node.ident, NodeKind::Constant, node.ident.span());
            syn::visit::visit_item_const(collector, node);
        });
    }

    fn visit_item_static(&mut self, node: &'ast syn::ItemStatic) {
        self.with_test_context(&node.attrs, |collector| {
            collector.add_symbol(&node.ident, NodeKind::Static, node.ident.span());
            syn::visit::visit_item_static(collector, node);
        });
    }

    fn visit_item_mod(&mut self, node: &'ast syn::ItemMod) {
        self.with_test_context(&node.attrs, |collector| {
            collector.add_symbol(&node.ident, NodeKind::Module, node.ident.span());
            if node.content.is_none() {
                // `mod x;` without a body pulls in x.rs or x/mod.rs. It is the
                // only thing that makes those files part of the crate, so without
                // this edge they look unreachable.
                collector.facts.imports.push(ImportFact::new(
                    format!("self::{}", node.ident),
                    source_span(collector.path, node.span()),
                ));
            }
            syn::visit::visit_item_mod(collector, node);
        });
    }

    fn visit_item_use(&mut self, node: &'ast syn::ItemUse) {
        for target in use_tree_targets(&node.tree) {
            let fact = ImportFact::new(target, source_span(self.path, node.span()));
            if matches!(node.vis, syn::Visibility::Inherited) {
                self.facts.imports.push(fact);
            } else {
                self.facts.reexports.push(fact);
            }
        }
        syn::visit::visit_item_use(self, node);
    }

    fn visit_expr_call(&mut self, node: &'ast syn::ExprCall) {
        if let Some(name) = callable_name(&node.func) {
            self.add_reference(name, node.span());
        }
        syn::visit::visit_expr_call(self, node);
    }

    fn visit_expr_method_call(&mut self, node: &'ast syn::ExprMethodCall) {
        self.add_reference(node.method.to_string(), node.span());
        if node.method == "route"
            && let Some((method, path)) = route_call(node)
        {
            self.add_endpoint(method, &path, node.span());
        }
        syn::visit::visit_expr_method_call(self, node);
    }
}

#[cfg(test)]
mod tests;
