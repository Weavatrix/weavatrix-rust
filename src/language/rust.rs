use super::{
    DomainFact, FileFacts, ImportFact, Language, LanguageAdapter, ReferenceFact, SourceFile,
    SymbolFact, SymbolLocator,
};
use crate::error::Result;
use crate::snapshot::Diagnostic;
use proc_macro2::{LineColumn, Span};
use syn::spanned::Spanned;
use syn::visit::Visit;
use syn::{Expr, Member, UseTree};
use weavatrix_graph::{EdgeKind, NodeKind, SourcePosition, SourceSpan};

use super::rust_endpoint::{attribute_routes, route_call};

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
            owners: Vec::new(),
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
    owners: Vec<SymbolLocator>,
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
        });
        locator
    }

    fn with_owner(&mut self, owner: SymbolLocator, visit: impl FnOnce(&mut Self)) {
        self.owners.push(owner);
        visit(self);
        self.owners.pop();
    }

    fn add_reference(&mut self, name: String, span: Span) {
        self.facts.references.push(ReferenceFact {
            name,
            kind: EdgeKind::Calls,
            span: source_span(self.path, span),
            owner: self.owners.last().cloned(),
        });
    }

    fn add_endpoint(&mut self, method: &str, path: &str, span: Span) {
        self.facts.domains.push(DomainFact {
            name: format!("{method} {path}"),
            kind: NodeKind::Endpoint,
            relation: EdgeKind::Exposes,
            span: source_span(self.path, span),
            owner: self.owners.last().cloned(),
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
        let owner = self.add_symbol(&node.sig.ident, NodeKind::Function, node.span());
        self.with_owner(owner, |collector| {
            collector.add_attribute_endpoints(&node.attrs);
            syn::visit::visit_item_fn(collector, node);
        });
    }

    fn visit_impl_item_fn(&mut self, node: &'ast syn::ImplItemFn) {
        let owner = self.add_symbol(&node.sig.ident, NodeKind::Method, node.span());
        self.with_owner(owner, |collector| {
            collector.add_attribute_endpoints(&node.attrs);
            syn::visit::visit_impl_item_fn(collector, node);
        });
    }

    fn visit_trait_item_fn(&mut self, node: &'ast syn::TraitItemFn) {
        let owner = self.add_symbol(&node.sig.ident, NodeKind::Method, node.span());
        self.with_owner(owner, |collector| {
            syn::visit::visit_trait_item_fn(collector, node);
        });
    }

    fn visit_item_struct(&mut self, node: &'ast syn::ItemStruct) {
        self.add_symbol(&node.ident, NodeKind::Struct, node.span());
        syn::visit::visit_item_struct(self, node);
    }

    fn visit_item_enum(&mut self, node: &'ast syn::ItemEnum) {
        self.add_symbol(&node.ident, NodeKind::Enum, node.span());
        syn::visit::visit_item_enum(self, node);
    }

    fn visit_item_trait(&mut self, node: &'ast syn::ItemTrait) {
        self.add_symbol(&node.ident, NodeKind::Trait, node.span());
        syn::visit::visit_item_trait(self, node);
    }

    fn visit_item_type(&mut self, node: &'ast syn::ItemType) {
        self.add_symbol(&node.ident, NodeKind::TypeAlias, node.span());
        syn::visit::visit_item_type(self, node);
    }

    fn visit_item_const(&mut self, node: &'ast syn::ItemConst) {
        self.add_symbol(&node.ident, NodeKind::Constant, node.span());
        syn::visit::visit_item_const(self, node);
    }

    fn visit_item_static(&mut self, node: &'ast syn::ItemStatic) {
        self.add_symbol(&node.ident, NodeKind::Static, node.span());
        syn::visit::visit_item_static(self, node);
    }

    fn visit_item_mod(&mut self, node: &'ast syn::ItemMod) {
        self.add_symbol(&node.ident, NodeKind::Module, node.span());
        if node.content.is_none() {
            // `mod x;` without a body pulls in x.rs or x/mod.rs. It is the
            // only thing that makes those files part of the crate, so without
            // this edge they look unreachable.
            self.facts.imports.push(ImportFact::new(
                format!("self::{}", node.ident),
                source_span(self.path, node.span()),
            ));
        }
        syn::visit::visit_item_mod(self, node);
    }

    fn visit_item_use(&mut self, node: &'ast syn::ItemUse) {
        self.facts.imports.push(ImportFact::new(
            use_tree_text(&node.tree),
            source_span(self.path, node.span()),
        ));
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

fn callable_name(expression: &Expr) -> Option<String> {
    match expression {
        Expr::Path(path) => path
            .path
            .segments
            .last()
            .map(|segment| segment.ident.to_string()),
        Expr::Field(field) => match &field.member {
            Member::Named(name) => Some(name.to_string()),
            Member::Unnamed(_) => None,
        },
        Expr::Group(group) => callable_name(&group.expr),
        Expr::Paren(parenthesized) => callable_name(&parenthesized.expr),
        _ => None,
    }
}

fn use_tree_text(tree: &UseTree) -> String {
    match tree {
        UseTree::Path(path) => format!("{}::{}", path.ident, use_tree_text(&path.tree)),
        UseTree::Name(name) => name.ident.to_string(),
        UseTree::Rename(rename) => format!("{} as {}", rename.ident, rename.rename),
        UseTree::Glob(_) => "*".into(),
        UseTree::Group(group) => {
            let items = group.items.iter().map(use_tree_text).collect::<Vec<_>>();
            format!("{{{}}}", items.join(","))
        }
    }
}

fn source_span(path: &str, span: Span) -> SourceSpan {
    SourceSpan {
        file: path.to_owned(),
        start: position(span.start()),
        end: position(span.end()),
    }
}

fn position(point: LineColumn) -> SourcePosition {
    SourcePosition {
        line: u32::try_from(point.line).unwrap_or(u32::MAX),
        column: u32::try_from(point.column)
            .unwrap_or(u32::MAX)
            .saturating_add(1),
    }
}

#[cfg(test)]
mod tests;
