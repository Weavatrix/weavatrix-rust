#![allow(dead_code)]

#[path = "language_fixture/mod.rs"]
mod language_fixture;
#[cfg(all(
    feature = "clone",
    feature = "git",
    feature = "lang-rust",
    feature = "memory",
    feature = "search",
    feature = "semantic",
    feature = "vector"
))]
#[path = "read_only_contract/mod.rs"]
mod read_only_contract;
#[path = "support/mod.rs"]
mod support;
#[path = "tool_fixture/mod.rs"]
mod tool_fixture;

#[path = "tools/ci_restrictions/mod.rs"]
mod ci_restrictions;
#[path = "tools/config_identity.rs"]
mod config_identity;
#[path = "tools/graph_evidence/mod.rs"]
mod graph_evidence;
#[path = "tools/tool_catalog_parity.rs"]
mod tool_catalog_parity;
#[path = "tools/tool_contract_architecture.rs"]
mod tool_contract_architecture;
#[path = "tools/tool_contract_audit.rs"]
mod tool_contract_audit;
#[path = "tools/tool_contract_read_only.rs"]
mod tool_contract_read_only;
#[path = "tools/tool_contract_transport.rs"]
mod tool_contract_transport;
#[path = "tools/tool_dependency_audit.rs"]
mod tool_dependency_audit;
#[path = "tools/tool_health_parity.rs"]
mod tool_health_parity;
#[path = "tools/tool_parameter_contracts.rs"]
mod tool_parameter_contracts;
#[path = "tools/tool_runtime_parity.rs"]
mod tool_runtime_parity;
