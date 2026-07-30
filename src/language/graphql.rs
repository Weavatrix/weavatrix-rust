//! GraphQL graph adapter over `weavatrix-parse`'s lossless typed facts.

use super::contract::{add_symbol, facts_with_diagnostics, source_span};
use super::{
    DomainFact, FileFacts, Language, LanguageAdapter, ReferenceFact, SourceFile, SymbolLocator,
};
use crate::model::{Error, Result};
use std::collections::BTreeMap;
use weavatrix_graph::{EdgeKind, NodeKind};
use weavatrix_parse::{Contract, ContractKind, GraphqlOperation, GraphqlType};

#[derive(Debug, Clone, Copy)]
pub struct GraphqlAdapter;

impl LanguageAdapter for GraphqlAdapter {
    fn language(&self) -> Language {
        Language::Graphql
    }

    fn extensions(&self) -> &'static [&'static str] {
        &["graphql", "gql"]
    }

    fn extractor(&self) -> &'static str {
        "weavatrix.parse.graphql"
    }

    fn parse(&self, source: SourceFile<'_>) -> Result<FileFacts> {
        let parsed = weavatrix_parse::extract(source.text, weavatrix_parse::Language::Graphql);
        let mut facts = facts_with_diagnostics(source.path, &parsed.diagnostics);
        if !facts.diagnostics.is_empty() {
            return Ok(facts);
        }

        let mut owners = BTreeMap::new();
        for contract in parsed.contracts {
            apply_contract(source.path, &mut facts, &mut owners, contract)?;
        }
        Ok(facts)
    }
}

fn apply_contract(
    path: &str,
    facts: &mut FileFacts,
    owners: &mut BTreeMap<String, SymbolLocator>,
    contract: Contract,
) -> Result<()> {
    let Contract {
        name,
        kind,
        span: contract_span,
        owner,
    } = contract;
    let span = source_span(path, &contract_span);
    match kind {
        ContractKind::GraphqlType(kind) => {
            add_symbol(facts, owners, name, graphql_type(kind), span, None);
        }
        ContractKind::GraphqlField {
            operation,
            return_type,
        } => add_field(
            path,
            facts,
            owners,
            &name,
            owner.as_deref(),
            contract_span,
            span,
            operation,
            &return_type,
        )?,
        ContractKind::GraphqlOperation(_) => {
            add_symbol(facts, owners, name, NodeKind::Function, span, None);
        }
        ContractKind::GraphqlCall(operation) => {
            facts.domains.push(DomainFact {
                name: format!("GRAPHQL {} {name}", operation_name(operation)),
                kind: NodeKind::Endpoint,
                relation: EdgeKind::Calls,
                span,
                owner: owner.as_ref().and_then(|name| owners.get(name)).cloned(),
            });
        }
        ContractKind::GraphqlFragment { .. } => {
            add_symbol(
                facts,
                owners,
                name,
                NodeKind::Custom("graphql_fragment".to_owned()),
                span,
                None,
            );
        }
        ContractKind::GraphqlFragmentSpread => {
            facts.references.push(ReferenceFact {
                name,
                kind: EdgeKind::References,
                receiver: None,
                qualified: false,
                span,
                owner: owner.as_ref().and_then(|name| owners.get(name)).cloned(),
            });
        }
        _ => {}
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn add_field(
    path: &str,
    facts: &mut FileFacts,
    owners: &BTreeMap<String, SymbolLocator>,
    name: &str,
    owner_name: Option<&str>,
    contract_span: weavatrix_parse::Span,
    span: weavatrix_graph::SourceSpan,
    operation: Option<GraphqlOperation>,
    return_type: &str,
) -> Result<()> {
    let owner_name = owner_name.ok_or_else(|| Error::Parse {
        language: "graphql",
        path: path.to_owned(),
        message: format!(
            "field {} has no declaring type at {}:{}",
            name, contract_span.line, contract_span.column
        ),
    })?;
    let owner = owners
        .get(owner_name)
        .cloned()
        .ok_or_else(|| Error::Parse {
            language: "graphql",
            path: path.to_owned(),
            message: format!(
                "field {} refers to undeclared owner {owner_name} at {}:{}",
                name, contract_span.line, contract_span.column
            ),
        })?;
    let name = operation.map_or_else(
        || format!("GRAPHQL FIELD {owner_name}.{name}"),
        |kind| format!("GRAPHQL {} {name}", operation_name(kind)),
    );
    facts.domains.push(DomainFact {
        name,
        kind: operation.map_or_else(
            || NodeKind::Custom("graphql_field".to_owned()),
            |_| NodeKind::Endpoint,
        ),
        relation: operation.map_or(EdgeKind::Contains, |_| EdgeKind::Exposes),
        span: span.clone(),
        owner: Some(owner.clone()),
    });
    if let Some(target) = referenced_type(return_type) {
        facts.references.push(ReferenceFact {
            name: target,
            kind: EdgeKind::References,
            receiver: None,
            qualified: false,
            span,
            owner: Some(owner),
        });
    }
    Ok(())
}

fn graphql_type(kind: GraphqlType) -> NodeKind {
    match kind {
        GraphqlType::Object | GraphqlType::Input => NodeKind::Struct,
        GraphqlType::Interface => NodeKind::Trait,
        GraphqlType::Enum => NodeKind::Enum,
        GraphqlType::Scalar | GraphqlType::Union => NodeKind::TypeAlias,
    }
}

fn operation_name(operation: GraphqlOperation) -> &'static str {
    match operation {
        GraphqlOperation::Query => "QUERY",
        GraphqlOperation::Mutation => "MUTATION",
        GraphqlOperation::Subscription => "SUBSCRIPTION",
    }
}

fn referenced_type(type_name: &str) -> Option<String> {
    let name = type_name
        .trim_matches(|character| matches!(character, '[' | ']' | '!'))
        .to_owned();
    (!name.is_empty() && !matches!(name.as_str(), "ID" | "String" | "Int" | "Float" | "Boolean"))
        .then_some(name)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn converts_schema_operations_fragments_and_type_references() {
        let source = concat!(
            "type Query { user: User }\n",
            "type User { id: ID! }\n",
            "fragment Root on Query { user { id } }\n",
            "query Get { ...Root }\n",
        );
        let facts = GraphqlAdapter
            .parse(SourceFile {
                path: "schema.graphql",
                text: source,
            })
            .unwrap();
        assert!(facts.diagnostics.is_empty());
        assert!(facts.domains.iter().any(|fact| {
            fact.name == "GRAPHQL QUERY user" && fact.relation == EdgeKind::Exposes
        }));
        assert!(
            facts.domains.iter().any(|fact| {
                fact.name == "GRAPHQL QUERY user" && fact.relation == EdgeKind::Calls
            })
        );
        assert!(facts.references.iter().any(|fact| fact.name == "User"));
        assert!(facts.references.iter().any(|fact| fact.name == "Root"));
    }
}
