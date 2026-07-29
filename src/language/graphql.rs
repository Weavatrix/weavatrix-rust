//! GraphQL graph adapter over `weavatrix-parse`'s lossless typed facts.

use super::contract::{add_symbol, facts_with_diagnostics, source_span};
use super::{DomainFact, FileFacts, Language, LanguageAdapter, ReferenceFact, SourceFile};
use crate::Result;
use crate::error::Error;
use std::collections::BTreeMap;
use weavatrix_graph::{EdgeKind, NodeKind};
use weavatrix_parse::{ContractKind, GraphqlOperation, GraphqlType};

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

    // One exhaustive typed-contract-to-graph mapping is easier to audit than
    // state split across callbacks for each GraphQL construct.
    #[allow(clippy::too_many_lines)]
    fn parse(&self, source: SourceFile<'_>) -> Result<FileFacts> {
        let parsed = weavatrix_parse::extract(source.text, weavatrix_parse::Language::Graphql);
        let mut facts = facts_with_diagnostics(source.path, &parsed.diagnostics);
        if !facts.diagnostics.is_empty() {
            return Ok(facts);
        }

        let mut owners = BTreeMap::new();
        for contract in parsed.contracts {
            let contract_span = source_span(source.path, &contract.span);
            match contract.kind {
                ContractKind::GraphqlType(kind) => {
                    add_symbol(
                        &mut facts,
                        &mut owners,
                        contract.name,
                        graphql_type(kind),
                        contract_span,
                        None,
                    );
                }
                ContractKind::GraphqlField {
                    operation,
                    return_type,
                } => {
                    let owner_name = contract.owner.as_deref().ok_or_else(|| Error::Parse {
                        language: "graphql",
                        path: source.path.to_owned(),
                        message: format!(
                            "field {} has no declaring type at {}:{}",
                            contract.name, contract.span.line, contract.span.column
                        ),
                    })?;
                    let owner = owners
                        .get(owner_name)
                        .cloned()
                        .ok_or_else(|| Error::Parse {
                            language: "graphql",
                            path: source.path.to_owned(),
                            message: format!(
                                "field {} refers to undeclared owner {owner_name} at {}:{}",
                                contract.name, contract.span.line, contract.span.column
                            ),
                        })?;
                    let name = operation.map_or_else(
                        || format!("GRAPHQL FIELD {owner_name}.{}", contract.name),
                        |operation| {
                            format!("GRAPHQL {} {}", operation_name(operation), contract.name)
                        },
                    );
                    facts.domains.push(DomainFact {
                        name,
                        kind: operation.map_or_else(
                            || NodeKind::Custom("graphql_field".to_owned()),
                            |_| NodeKind::Endpoint,
                        ),
                        relation: operation.map_or(EdgeKind::Contains, |_| EdgeKind::Exposes),
                        span: contract_span.clone(),
                        owner: Some(owner.clone()),
                    });
                    if let Some(target) = referenced_type(&return_type) {
                        facts.references.push(ReferenceFact {
                            name: target,
                            kind: EdgeKind::References,
                            receiver: None,
                            qualified: false,
                            span: contract_span,
                            owner: Some(owner),
                        });
                    }
                }
                ContractKind::GraphqlOperation(operation) => {
                    add_symbol(
                        &mut facts,
                        &mut owners,
                        contract.name,
                        NodeKind::Function,
                        contract_span,
                        None,
                    );
                    let _ = operation;
                }
                ContractKind::GraphqlCall(operation) => {
                    facts.domains.push(DomainFact {
                        name: format!("GRAPHQL {} {}", operation_name(operation), contract.name),
                        kind: NodeKind::Endpoint,
                        relation: EdgeKind::Calls,
                        span: contract_span,
                        owner: contract
                            .owner
                            .as_ref()
                            .and_then(|name| owners.get(name))
                            .cloned(),
                    });
                }
                ContractKind::GraphqlFragment { .. } => {
                    add_symbol(
                        &mut facts,
                        &mut owners,
                        contract.name,
                        NodeKind::Custom("graphql_fragment".to_owned()),
                        contract_span,
                        None,
                    );
                }
                ContractKind::GraphqlFragmentSpread => {
                    facts.references.push(ReferenceFact {
                        name: contract.name,
                        kind: EdgeKind::References,
                        receiver: None,
                        qualified: false,
                        span: contract_span,
                        owner: contract
                            .owner
                            .as_ref()
                            .and_then(|name| owners.get(name))
                            .cloned(),
                    });
                }
                _ => {}
            }
        }
        Ok(facts)
    }
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
