//! Bounded offline API and event-transport contract analysis.

use crate::engine::RepositoryState;
use crate::operations::{optional_str, optional_u64};
use blazingly_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Component, Path};
use weavatrix_graph::{AttributeValue, EdgeKind, NodeKind};
use weavatrix_parse::{Language, Token, TokenKind, tokenize};

mod amqp;
mod arguments;
mod aws;
mod bindings;
mod detection;
mod event;
mod fallback;
mod http;
mod jms;
mod kafka;
mod matching;
mod model;
mod nats;
mod runtime;
mod runtime_normalization;
mod scan;
mod syntax;
mod time;
mod typed;
mod typed_matching;

use amqp::detect_amqp;
use arguments::{
    DEFAULT_MAX_FILE_BYTES, DEFAULT_MAX_FILES, DEFAULT_MAX_OBSERVATIONS, DEFAULT_PER_CONTRACT,
    DEFAULT_RUNTIME_REPORTS, MAX_MAX_FILE_BYTES, MAX_MAX_FILES, MAX_MAX_OBSERVATIONS,
    MAX_PER_CONTRACT, MAX_RUNTIME_OBSERVATIONS, MAX_RUNTIME_REPORT_BYTES, RUNTIME_SCHEMA,
    bounded_u64, bounded_usize,
};
use aws::{destination_entity, detect_aws};
use bindings::{bindings_none, remember_binding, remember_transport_origin};
use detection::{detect, is_import_keyword, propagate_assignment_binding, provider_text_matches};
use fallback::{add_graph_fallbacks, observation_identity};
use jms::detect_jms;
use kafka::detect_kafka;
use matching::{Evaluation, evaluate};
use model::{
    Bindings, Call, Certainty, ContractKey, Detection, Entity, Observation, ProviderHints, Role,
    ScanSummary, SourceScan, Transport,
};
use nats::detect_nats;
use runtime::{RuntimeAggregate, load_and_merge_runtime};
use runtime_normalization::{
    normalize_runtime_observation, otlp_event_observations, safe_repository_path,
};
use scan::{ScanLimits, language_for_path, scan_repository};
use syntax::{
    assigned_variable, call_chain, call_name_index, first_value, literal_value, matching_close,
    non_empty_resources, positional_identifier, positional_values, property, receiver_name,
    resource_values, source_line,
};
use time::timestamp_millis;
use typed_matching::{
    typed_client_relations, typed_diagnostics, typed_evidence, typed_identity, typed_key,
    typed_mismatch_kind, typed_nodes,
};

pub(super) use event::event_contracts;
pub(super) use http::http_contracts;
pub(super) use typed::{empty_typed_contracts, typed_api_contracts};

#[cfg(test)]
#[path = "../../../tests/support/transport_fixture.rs"]
mod transport_fixture;

#[cfg(test)]
mod tests;
