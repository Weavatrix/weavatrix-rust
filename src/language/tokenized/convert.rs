use super::domains::{class_route_prefixes, domain};
use super::kinds::{edge_kind, node_kind, span};
use crate::language::{
    FileFacts, ImportBindingFact, ImportFact, ReferenceFact, SymbolFact, SymbolLocator,
};
use weavatrix_graph::EdgeKind;
use weavatrix_parse::{DeclarationKind, Facts};

fn owner_is_type(facts: &Facts, name: &str) -> bool {
    facts.declarations.iter().any(|declaration| {
        declaration.name == name
            && matches!(
                declaration.kind,
                DeclarationKind::Class
                    | DeclarationKind::Struct
                    | DeclarationKind::Enum
                    | DeclarationKind::Interface
                    | DeclarationKind::Trait
            )
    })
}

fn is_swift_source(path: &str) -> bool {
    std::path::Path::new(path)
        .extension()
        .is_some_and(|ext| ext.eq_ignore_ascii_case("swift"))
}

fn keep_declaration(facts: &Facts, path: &str, kind: DeclarationKind, owner: Option<&str>) -> bool {
    if !is_swift_source(path) {
        return true;
    }
    if !matches!(kind, DeclarationKind::Constant | DeclarationKind::Variable) {
        return true;
    }
    owner.is_some_and(|name| owner_is_type(facts, name))
}

/// Converts parser facts into the language-neutral graph-builder contract.
pub(super) fn convert(facts: &Facts, path: &str) -> FileFacts {
    let mut converted = FileFacts::default();
    let class_route_prefixes = class_route_prefixes(facts);

    for declaration in &facts.declarations {
        if !keep_declaration(facts, path, declaration.kind, declaration.owner.as_deref()) {
            continue;
        }
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
        let owner = reference.owner.as_ref().and_then(|name| {
            facts
                .declarations
                .iter()
                .find(|declaration| declaration.name == *name)
                .map(|declaration| SymbolLocator {
                    name: declaration.name.clone(),
                    kind: node_kind(declaration.kind),
                    span: span(&declaration.span, path),
                })
        });
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
            owner: owner.clone(),
        });
        // A bare name passed to another call is runtime use even though the
        // callee, rather than this call site, may invoke it. The lossless
        // parser preserves these bindings (`register(handler)`,
        // `router.get(path, handler)`); keep them as reference evidence rather
        // than incorrectly classifying the supplied function as dead code.
        converted
            .references
            .extend(reference.name_arguments.iter().map(|name| ReferenceFact {
                name: name.clone(),
                kind: EdgeKind::References,
                receiver: None,
                qualified: false,
                span: span(&reference.span, path),
                owner: owner.clone(),
            }));
    }

    converted
}
