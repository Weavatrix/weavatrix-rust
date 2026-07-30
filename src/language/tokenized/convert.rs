use super::domains::{class_route_prefixes, domain};
use super::kinds::{edge_kind, node_kind, span};
use crate::language::{
    FileFacts, ImportBindingFact, ImportFact, ReferenceFact, SymbolFact, SymbolLocator,
};
use weavatrix_parse::Facts;

/// Converts parser facts into the language-neutral graph-builder contract.
pub(super) fn convert(facts: &Facts, path: &str) -> FileFacts {
    let mut converted = FileFacts::default();
    let class_route_prefixes = class_route_prefixes(facts);

    for declaration in &facts.declarations {
        converted.symbols.push(SymbolFact {
            name: declaration.name.clone(),
            kind: node_kind(declaration.kind),
            span: span(&declaration.span, path),
            test_only: facts.declaration_is_test_only(declaration.span),
            owner: declaration.owner.clone(),
        });
    }

    for import in &facts.imports {
        let bindings = import
            .bindings
            .iter()
            .map(|binding| ImportBindingFact {
                imported: binding.imported.clone(),
                local: binding.local.clone(),
            })
            .collect();
        let fact = if import.type_only {
            ImportFact::type_only(import.specifier.clone(), span(&import.span, path))
        } else {
            ImportFact::new(import.specifier.clone(), span(&import.span, path))
        }
        .with_bindings(bindings);
        if import.reexport {
            converted.reexports.push(fact);
        } else {
            converted.imports.push(fact);
        }
    }

    for reference in &facts.references {
        domain(
            reference,
            path,
            facts,
            &class_route_prefixes,
            &mut converted,
        );
        converted.references.push(ReferenceFact {
            name: reference.name.clone(),
            kind: edge_kind(reference.kind),
            receiver: reference.receiver.clone(),
            qualified: reference.receiver.is_some(),
            span: span(&reference.span, path),
            owner: reference.owner.as_ref().and_then(|name| {
                facts
                    .declarations
                    .iter()
                    .find(|declaration| declaration.name == *name)
                    .map(|declaration| SymbolLocator {
                        name: declaration.name.clone(),
                        kind: node_kind(declaration.kind),
                        span: span(&declaration.span, path),
                    })
            }),
        });
    }

    converted
}
