#![allow(dead_code)]

#[path = "agent_support/mod.rs"]
mod agent_support;
#[path = "dify_support/mod.rs"]
mod dify_support;
#[path = "n8n_support/mod.rs"]
mod n8n_support;

#[path = "domains/agent_contracts.rs"]
mod agent_contracts;
#[path = "domains/agent_plugins.rs"]
mod agent_plugins;
#[path = "domains/agent_schema.rs"]
mod agent_schema;
#[path = "domains/agent_skills.rs"]
mod agent_skills;
#[path = "domains/dify_apps.rs"]
mod dify_apps;
#[path = "domains/dify_variables.rs"]
mod dify_variables;
#[path = "domains/n8n_array_documents.rs"]
mod n8n_array_documents;
#[path = "domains/n8n_expressions.rs"]
mod n8n_expressions;
#[path = "domains/n8n_limits.rs"]
mod n8n_limits;
#[path = "domains/n8n_workflows.rs"]
mod n8n_workflows;
