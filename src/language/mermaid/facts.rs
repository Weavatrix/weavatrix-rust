use super::model::{
    Completeness, Diagram, declared_architecture, diagram_kind, element_kind, group_kind,
};
use crate::language::{
    BoundEdgeFact, DomainFact, FileFacts, ReferenceFact, SymbolFact, SymbolLocator,
};
use std::collections::BTreeMap;
use weavatrix_graph::{EdgeKind, NodeKind, SourceSpan};

pub(super) fn to_file_facts(path: &str, diagrams: &[Diagram]) -> FileFacts {
    let mut facts = FileFacts::default();
    for diagram in diagrams {
        emit_diagram(&mut facts, path, diagram);
    }
    facts
}

fn emit_diagram(facts: &mut FileFacts, path: &str, diagram: &Diagram) {
    facts.diagnostics.extend(diagram.diagnostics.iter().cloned());
    let diagram_symbol = symbol(&diagram.key, diagram_kind(), diagram.span.clone());
    let owner = locator(&diagram_symbol);
    facts.symbols.push(diagram_symbol);
    push_meta(
        facts,
        &owner,
        &diagram.span,
        &[
            format!("kind:{}", diagram.kind),
            format!("direction:{}", empty_or(&diagram.direction, "unspecified")),
            format!("completeness:{}", diagram.completeness.as_str()),
            format!("source:{path}"),
        ],
    );
    let mut locators = BTreeMap::<String, SymbolLocator>::new();
    locators.insert(diagram.key.clone(), owner.clone());
    for group in &diagram.groups {
        let group_symbol = symbol(&group.id, group_kind(), group.span.clone());
        locators.insert(group.id.clone(), locator(&group_symbol));
        facts
            .references
            .push(contains(&owner, &group.id, group.span.clone()));
        facts.symbols.push(group_symbol);
        push_named(
            facts,
            &owner,
            &group.span,
            &format!("group_title:{}", group.title),
            NodeKind::Unknown,
            EdgeKind::Configures,
        );
    }
    for element in &diagram.elements {
        let element_symbol = symbol(&element.id, element_kind(), element.span.clone());
        locators.insert(element.id.clone(), locator(&element_symbol));
        facts
            .references
            .push(contains(&owner, &element.id, element.span.clone()));
        facts.symbols.push(element_symbol);
        push_named(
            facts,
            locators.get(&element.id).unwrap_or(&owner),
            &element.span,
            &format!("label:{}", element.label),
            NodeKind::Unknown,
            EdgeKind::Configures,
        );
        push_named(
            facts,
            locators.get(&element.id).unwrap_or(&owner),
            &element.span,
            &format!("native_id:{}", element.id),
            NodeKind::Unknown,
            EdgeKind::Configures,
        );
        if let Some(group) = &element.group
            && let Some(group_loc) = locators.get(group)
        {
            facts.references.push(contains(
                group_loc,
                &element.id,
                element.span.clone(),
            ));
        }
        for occurrence in &element.occurrences {
            push_named(
                facts,
                locators.get(&element.id).unwrap_or(&owner),
                occurrence,
                &format!("occurrence:{}:{}", occurrence.start.line, occurrence.start.column),
                NodeKind::Unknown,
                EdgeKind::Configures,
            );
        }
    }
    for relation in &diagram.relations {
        emit_relation(facts, &locators, relation);
    }
    let known = diagram.elements.len();
    let total = known + usize::from(diagram.completeness == Completeness::Partial);
    push_named(
        facts,
        &owner,
        &diagram.span,
        &format!("coverage:elements:{known}/{total}"),
        NodeKind::Unknown,
        EdgeKind::Configures,
    );
}

fn emit_relation(
    facts: &mut FileFacts,
    locators: &BTreeMap<String, SymbolLocator>,
    relation: &super::model::Relation,
) {
    let Some(from) = locators.get(&relation.from) else {
        return;
    };
    let Some(to) = locators.get(&relation.to) else {
        return;
    };
    facts.bound_edges.push(BoundEdgeFact {
        from: from.clone(),
        to: to.clone(),
        kind: declared_architecture(),
        span: relation.span.clone(),
        detail: format!(
            "{}|{}|{}",
            relation.marker, relation.label, relation.occurrence
        ),
    });
}

fn empty_or<'a>(value: &'a str, fallback: &'a str) -> &'a str {
    if value.is_empty() { fallback } else { value }
}

fn symbol(name: &str, kind: NodeKind, span: SourceSpan) -> SymbolFact {
    SymbolFact {
        name: name.to_owned(),
        kind,
        span,
        test_only: false,
        exported: false,
        source_fingerprint: None,
        source_extent: None,
        owner: None,
    }
}

fn locator(symbol: &SymbolFact) -> SymbolLocator {
    SymbolLocator {
        name: symbol.name.clone(),
        kind: symbol.kind.clone(),
        span: symbol.span.clone(),
    }
}

fn contains(owner: &SymbolLocator, name: &str, span: SourceSpan) -> ReferenceFact {
    ReferenceFact {
        name: name.to_owned(),
        kind: EdgeKind::Contains,
        receiver: None,
        qualified: false,
        span,
        owner: Some(owner.clone()),
    }
}

fn push_meta(facts: &mut FileFacts, owner: &SymbolLocator, span: &SourceSpan, names: &[String]) {
    for name in names {
        push_named(facts, owner, span, name, NodeKind::Unknown, EdgeKind::Configures);
    }
}

fn push_named(
    facts: &mut FileFacts,
    owner: &SymbolLocator,
    span: &SourceSpan,
    name: &str,
    kind: NodeKind,
    relation: EdgeKind,
) {
    facts.domains.push(DomainFact {
        name: name.to_owned(),
        kind,
        relation,
        span: span.clone(),
        owner: Some(owner.clone()),
    });
}
