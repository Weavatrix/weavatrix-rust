use super::contracts;
use super::model::{ContractResults, EvidencePage, TraceSummary};
use crate::engine::RepositoryState;
use blazingly_json::{Value, json};
use std::collections::BTreeSet;

pub(super) fn summarize(results: &ContractResults) -> TraceSummary {
    let http_mismatches = total(&results.http, "method_mismatches");
    let event_mismatches = total(&results.events, "mismatches");
    let typed_mismatches =
        total(&results.graphql, "mismatches") + total(&results.grpc, "mismatches");
    let matched = [
        &results.http,
        &results.events,
        &results.graphql,
        &results.grpc,
    ]
    .into_iter()
    .map(|result| total(result, "matches"))
    .sum();
    let verdict = if typed_mismatches > 0 {
        "TYPED_API_CONTRACT_MISMATCH"
    } else if event_mismatches > 0 {
        "EVENT_CONTRACT_MISMATCH"
    } else if http_mismatches > 0 {
        "HTTP_METHOD_MISMATCH"
    } else if matched > 0 {
        "MATCHED"
    } else {
        "NO_STATIC_CLIENT_MATCH"
    };
    TraceSummary {
        verdict,
        http_mismatches,
        event_mismatches,
        typed_mismatches,
        matched,
    }
}

pub(super) fn build(
    backend: &RepositoryState,
    clients: &BTreeSet<&str>,
    results: &ContractResults,
    page: &EvidencePage,
    reasons: &[String],
    summary: &TraceSummary,
) -> Value {
    let has_more = page.end < page.total_items;
    let next_cursor = has_more.then(|| format!("v1:{}", page.end));
    let matched_contracts = contracts::matches(results);
    let unmatched = contracts::unmatched_endpoints(results);
    json!({
        "crossRepoContractV": 1,
        "status": "COMPLETE",
        "verdict": {
            "code": summary.verdict,
            "method_mismatches": summary.http_mismatches,
            "event_mismatches": summary.event_mismatches,
            "typed_contract_mismatches": summary.typed_mismatches,
            "matched_contracts": summary.matched
        },
        "repositories": {
            "backend": backend.root(),
            "clients": clients
        },
        "transport": results.transport,
        "http": results.http,
        "graphql": results.graphql,
        "grpc": results.grpc,
        "transport_contracts": results.events,
        "matches": matched_contracts,
        "unmatched_endpoints": unmatched,
        "evidencePage": {
            "detail": page.detail,
            "offset": page.offset,
            "page_size": page.page_size,
            "total_items": page.total_items,
            "returned_items": page.items.len(),
            "has_more": has_more,
            "next_cursor": next_cursor,
            "items": page.items
        },
        "completeness": {
            "complete": reasons.is_empty(),
            "status": "COMPLETE",
            "reasons": reasons
        },
        "precision": "lossless-parser-derived GraphQL/protobuf contracts, exact typed graph matches, event semantics, and exact or template-prefix HTTP literals",
        "dynamic_contracts": {
            "evaluated": true,
            "method": "bounded static candidates plus revision-bound runtime evidence when supplied"
        },
        "source_mutation": "NONE",
        "network": "NONE"
    })
}

fn total(value: &Value, key: &str) -> u64 {
    value["totals"][key].as_u64().unwrap_or(0)
}
