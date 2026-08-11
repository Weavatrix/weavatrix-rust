use super::syntax::scoped_use_target;
use crate::language::{FileFacts, SymbolLocator};
use std::collections::BTreeMap;

#[derive(Default)]
pub(super) struct ModuleScope {
    inline: Vec<String>,
    path_modules: BTreeMap<String, String>,
}

impl ModuleScope {
    pub(super) fn for_file(file: &syn::File) -> Self {
        let path_modules = file
            .items
            .iter()
            .filter_map(|item| match item {
                syn::Item::Mod(module) if module.content.is_none() => module_path(module),
                _ => None,
            })
            .collect();
        Self {
            inline: Vec::new(),
            path_modules,
        }
    }

    pub(super) fn enter(&mut self, name: String) {
        self.inline.push(name);
    }

    pub(super) fn leave(&mut self) {
        self.inline.pop();
    }

    pub(super) fn target(&self, target: &str) -> String {
        let scoped = scoped_use_target(target, &self.inline);
        if !self.inline.is_empty() {
            return scoped;
        }
        let local = scoped.strip_prefix("self::").unwrap_or(&scoped);
        let alias = local.split("::").next().unwrap_or(local);
        self.path_modules.get(alias).cloned().unwrap_or(scoped)
    }

    pub(super) fn declared_target(&self, name: &str) -> String {
        self.path_modules
            .get(name)
            .cloned()
            .unwrap_or_else(|| self.target(&format!("self::{name}")))
    }
}

fn module_path(module: &syn::ItemMod) -> Option<(String, String)> {
    let path = module.attrs.iter().find_map(|attribute| {
        if !attribute.path().is_ident("path") {
            return None;
        }
        let syn::Meta::NameValue(meta) = &attribute.meta else {
            return None;
        };
        let syn::Expr::Lit(literal) = &meta.value else {
            return None;
        };
        let syn::Lit::Str(path) = &literal.lit else {
            return None;
        };
        Some(path.value())
    })?;
    let relative = if path.starts_with(['.', '/']) {
        path
    } else {
        format!("./{path}")
    };
    Some((module.ident.to_string(), relative))
}

#[derive(Clone, Default)]
pub(super) struct OwnerScope {
    pub(super) symbol: Option<SymbolLocator>,
    pub(super) type_name: Option<String>,
}

pub(super) enum OwnerUpdate {
    Symbol(SymbolLocator),
    Type(String),
}

impl OwnerScope {
    pub(super) fn apply(&mut self, update: OwnerUpdate) -> Self {
        let previous = self.clone();
        match update {
            OwnerUpdate::Symbol(owner) => self.symbol = Some(owner),
            OwnerUpdate::Type(owner) => self.type_name = Some(owner),
        }
        previous
    }
}

pub(super) fn sort_facts(facts: &mut FileFacts) {
    facts
        .symbols
        .sort_by(|left, right| left.span.cmp(&right.span));
    facts
        .references
        .sort_by(|left, right| left.span.cmp(&right.span));
    facts
        .imports
        .sort_by(|left, right| left.span.cmp(&right.span));
}
