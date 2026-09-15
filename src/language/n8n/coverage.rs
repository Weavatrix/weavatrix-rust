use super::model::{WorkflowRecord, workflow_kind};
use crate::language::{DomainFact, FileFacts, SymbolLocator};
use weavatrix_graph::{EdgeKind, NodeKind};

pub(super) fn emit(facts: &mut FileFacts, workflow: &WorkflowRecord) {
    let coverage = &workflow.coverage;
    facts.domains.push(ratio(
        workflow,
        format!(
            "coverage:structure:{}/{}",
            coverage.structure_accepted, coverage.structure_seen
        ),
    ));
    facts.domains.push(ratio(
        workflow,
        format!(
            "coverage:nodeSemantics:{}/{}",
            coverage.semantics_supported, coverage.semantics_seen
        ),
    ));
    facts.domains.push(ratio(
        workflow,
        format!(
            "coverage:expressions:{}/{}",
            coverage.expressions_resolved, coverage.expressions_seen
        ),
    ));
    facts
        .domains
        .push(ratio(workflow, "coverage:runtime:false".into()));
}

fn ratio(workflow: &WorkflowRecord, name: String) -> DomainFact {
    DomainFact {
        name,
        kind: NodeKind::Unknown,
        relation: EdgeKind::Configures,
        span: workflow.span.clone(),
        owner: Some(SymbolLocator {
            name: workflow.name.clone(),
            kind: workflow_kind(),
            span: workflow.span.clone(),
        }),
    }
}
