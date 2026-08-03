use super::syntax::scoped_use_target;
use crate::language::{FileFacts, SymbolLocator};

#[derive(Default)]
pub(super) struct ModuleScope {
    inline: Vec<String>,
}

impl ModuleScope {
    pub(super) fn enter(&mut self, name: String) {
        self.inline.push(name);
    }

    pub(super) fn leave(&mut self) {
        self.inline.pop();
    }

    pub(super) fn target(&self, target: &str) -> String {
        scoped_use_target(target, &self.inline)
    }
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
