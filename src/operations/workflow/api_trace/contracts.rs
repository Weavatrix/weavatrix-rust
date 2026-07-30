use super::model::ContractResults;
use crate::engine::RepositoryState;
use crate::operations::optional_str;
use crate::operations::transport_contracts::{
    empty_typed_contracts, event_contracts, http_contracts, typed_api_contracts,
};
use blazingly_json::{Value, json};

pub(super) fn analyze(
    backend: &RepositoryState,
    clients: &[(String, &RepositoryState)],
    args: &Value,
) -> Result<ContractResults, String> {
    let transport = optional_str(args, "transport")?.unwrap_or("all");
    if !matches!(transport, "all" | "http" | "graphql" | "grpc" | "event") {
        return Err("transport must be all, http, graphql, grpc, or event".to_owned());
    }
    if args.get("runtime_config").is_some() {
        return Err(
            "runtime_config is not revision-bound evidence; use runtime_evidence_files".to_owned(),
        );
    }
    let http = if matches!(transport, "all" | "http") {
        http_contracts(backend, clients, args)?
    } else {
        empty_contracts(&json!({"endpoints": 0, "matches": 0, "method_mismatches": 0}))
    };
    let events = if matches!(transport, "all" | "event") {
        event_contracts(backend, clients, args)?
    } else {
        empty_contracts(&json!({"contracts": 0, "matches": 0}))
    };
    let graphql = typed_contract(transport, "graphql", backend, clients, args)?;
    let grpc = typed_contract(transport, "grpc", backend, clients, args)?;
    Ok(ContractResults {
        transport: transport.to_owned(),
        http,
        events,
        graphql,
        grpc,
    })
}

fn empty_contracts(totals: &Value) -> Value {
    json!({
        "status": "NOT_APPLICABLE",
        "totals": totals,
        "contracts": []
    })
}

fn typed_contract(
    requested: &str,
    transport: &str,
    backend: &RepositoryState,
    clients: &[(String, &RepositoryState)],
    args: &Value,
) -> Result<Value, String> {
    if matches!(requested, "all") || requested == transport {
        typed_api_contracts(backend, clients, args, transport)
    } else {
        Ok(empty_typed_contracts(transport))
    }
}

pub(super) fn completeness_reasons(results: &ContractResults) -> Vec<String> {
    [&results.events, &results.graphql, &results.grpc]
        .into_iter()
        .flat_map(|result| {
            result
                .pointer("/completeness/reasons")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
        })
        .filter_map(Value::as_str)
        .map(str::to_owned)
        .collect()
}

pub(super) fn evidence(results: &ContractResults) -> Vec<Value> {
    let mut evidence = contracts(&results.http)
        .chain(contracts(&results.events))
        .chain(contracts(&results.graphql))
        .chain(contracts(&results.grpc))
        .cloned()
        .collect::<Vec<_>>();
    evidence.sort_by(|left, right| {
        left["key"]
            .as_str()
            .unwrap_or_default()
            .cmp(right["key"].as_str().unwrap_or_default())
    });
    evidence
}

pub(super) fn matches(results: &ContractResults) -> Vec<Value> {
    contracts(&results.http)
        .chain(array(&results.events, "matches"))
        .chain(array(&results.graphql, "matches"))
        .chain(array(&results.grpc, "matches"))
        .cloned()
        .collect()
}

pub(super) fn unmatched_endpoints(results: &ContractResults) -> u64 {
    [&results.http, &results.graphql, &results.grpc]
        .into_iter()
        .map(|result| {
            result
                .pointer("/totals/unmatched_endpoints")
                .and_then(Value::as_u64)
                .unwrap_or(0)
        })
        .sum()
}

fn contracts(value: &Value) -> impl Iterator<Item = &Value> {
    array(value, "contracts")
}

fn array<'value>(value: &'value Value, key: &str) -> impl Iterator<Item = &'value Value> {
    value[key].as_array().into_iter().flatten()
}
