use super::{
    DomainFact, FileFacts, ImportFact, Language, LanguageAdapter, ReferenceFact, SourceFile,
    SymbolFact, SymbolLocator,
};
use crate::model::{Diagnostic, Result};
use syn::spanned::Spanned;
use syn::visit::Visit;
use weavatrix_graph::{EdgeKind, NodeKind};

use endpoints::{
    associated_owner_name, attribute_routes, bare_path_name, callable_name, route_call,
};
use module_scope::{ModuleScope, OwnerScope, OwnerUpdate, sort_facts};
use syntax::{attributes_mark_test, impl_owner, source_span, use_tree_targets};

mod collector;
mod endpoints;
mod macro_calls;
mod module_scope;
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
            module_scope: ModuleScope::for_file(&syntax),
        };
        collector.visit_file(&syntax);
        sort_facts(&mut collector.facts);
        Ok(collector.facts)
    }
}

struct Collector<'source> {
    path: &'source str,
    facts: FileFacts,
    owner: OwnerScope,
    test_context: bool,
    module_scope: ModuleScope,
}

fn is_public(visibility: &syn::Visibility) -> bool {
    matches!(visibility, syn::Visibility::Public(_))
}

impl<'ast> Visit<'ast> for Collector<'_> {
    fn visit_item_fn(&mut self, node: &'ast syn::ItemFn) {
        self.with_test_context(&node.attrs, |collector| {
            let owner = collector.add_symbol(
                &node.sig.ident,
                NodeKind::Function,
                node.span(),
                is_public(&node.vis),
            );
            collector.with_owner(OwnerUpdate::Symbol(owner), |collector| {
                collector.add_attribute_endpoints(&node.attrs);
                syn::visit::visit_item_fn(collector, node);
            });
        });
    }

    fn visit_impl_item_fn(&mut self, node: &'ast syn::ImplItemFn) {
        self.with_test_context(&node.attrs, |collector| {
            let type_name = collector.owner.type_name.clone();
            let owner = collector.add_symbol(
                &node.sig.ident,
                NodeKind::Method,
                node.span(),
                is_public(&node.vis),
            );
            collector.with_owner(OwnerUpdate::Symbol(owner), |collector| {
                if let Some(type_name) = type_name {
                    collector.add_reference(
                        type_name,
                        EdgeKind::References,
                        false,
                        node.sig.ident.span(),
                    );
                }
                collector.add_attribute_endpoints(&node.attrs);
                syn::visit::visit_impl_item_fn(collector, node);
            });
        });
    }

    fn visit_trait_item_fn(&mut self, node: &'ast syn::TraitItemFn) {
        self.with_test_context(&node.attrs, |collector| {
            let owner = collector.add_symbol(&node.sig.ident, NodeKind::Method, node.span(), false);
            collector.with_owner(OwnerUpdate::Symbol(owner), |collector| {
                syn::visit::visit_trait_item_fn(collector, node);
            });
        });
    }

    fn visit_item_struct(&mut self, node: &'ast syn::ItemStruct) {
        self.with_test_context(&node.attrs, |collector| {
            collector.add_symbol(
                &node.ident,
                NodeKind::Struct,
                node.span(),
                is_public(&node.vis),
            );
            syn::visit::visit_item_struct(collector, node);
        });
    }

    fn visit_item_enum(&mut self, node: &'ast syn::ItemEnum) {
        self.with_test_context(&node.attrs, |collector| {
            collector.add_symbol(
                &node.ident,
                NodeKind::Enum,
                node.span(),
                is_public(&node.vis),
            );
            syn::visit::visit_item_enum(collector, node);
        });
    }

    fn visit_item_trait(&mut self, node: &'ast syn::ItemTrait) {
        self.with_test_context(&node.attrs, |collector| {
            collector.add_symbol(
                &node.ident,
                NodeKind::Trait,
                node.span(),
                is_public(&node.vis),
            );
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
            collector.add_symbol(
                &node.ident,
                NodeKind::TypeAlias,
                node.span(),
                is_public(&node.vis),
            );
            syn::visit::visit_item_type(collector, node);
        });
    }

    fn visit_item_const(&mut self, node: &'ast syn::ItemConst) {
        self.with_test_context(&node.attrs, |collector| {
            collector.add_symbol(
                &node.ident,
                NodeKind::Constant,
                node.span(),
                is_public(&node.vis),
            );
            syn::visit::visit_item_const(collector, node);
        });
    }

    fn visit_item_static(&mut self, node: &'ast syn::ItemStatic) {
        self.with_test_context(&node.attrs, |collector| {
            collector.add_symbol(
                &node.ident,
                NodeKind::Static,
                node.span(),
                is_public(&node.vis),
            );
            syn::visit::visit_item_static(collector, node);
        });
    }

    fn visit_item_mod(&mut self, node: &'ast syn::ItemMod) {
        self.with_test_context(&node.attrs, |collector| {
            collector.add_symbol(
                &node.ident,
                NodeKind::Module,
                node.span(),
                is_public(&node.vis),
            );
            if node.content.is_some() {
                collector.module_scope.enter(node.ident.to_string());
            } else {
                // `mod x;` pulls in x.rs or x/mod.rs; keep those files reachable.
                let target = collector
                    .module_scope
                    .declared_target(&node.ident.to_string());
                collector.facts.imports.push(ImportFact::new(
                    target,
                    source_span(collector.path, node.span()),
                ));
            }
            syn::visit::visit_item_mod(collector, node);
            if node.content.is_some() {
                collector.module_scope.leave();
            }
        });
    }

    fn visit_item_use(&mut self, node: &'ast syn::ItemUse) {
        for target in use_tree_targets(&node.tree) {
            let target = self.module_scope.target(&target);
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
            self.add_reference(name, EdgeKind::Calls, false, node.span());
        }
        if let Some(name) = associated_owner_name(&node.func) {
            self.add_reference(name, EdgeKind::References, false, node.func.span());
        }
        syn::visit::visit_expr_call(self, node);
    }

    fn visit_expr_method_call(&mut self, node: &'ast syn::ExprMethodCall) {
        self.add_reference(node.method.to_string(), EdgeKind::Calls, false, node.span());
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
        syn::visit::visit_expr_method_call(self, node);
    }

    fn visit_expr_macro(&mut self, node: &'ast syn::ExprMacro) {
        macro_calls::for_each_standard_argument(node, |argument| self.visit_expr(argument));
        syn::visit::visit_expr_macro(self, node);
    }

    fn visit_type_path(&mut self, node: &'ast syn::TypePath) {
        // Never bind a qualified path by its final segment alone.
        if node.qself.is_none()
            && node.path.segments.len() == 1
            && let Some(segment) = node.path.segments.last()
        {
            self.add_reference(
                segment.ident.to_string(),
                EdgeKind::References,
                false,
                node.span(),
            );
        }
        syn::visit::visit_type_path(self, node);
    }
}
#[cfg(test)]
mod tests;
