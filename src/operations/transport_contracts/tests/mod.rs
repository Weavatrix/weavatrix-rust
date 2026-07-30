//! Event-transport contract regression tests.

use super::matching::routing_keys_match;
use super::runtime::load_runtime_evidence;
use super::scan::{extract_observations, may_contain_transport_observation};
use super::time::parse_rfc3339_millis;
use super::{Certainty, Entity, Role, Transport, event_contracts};
use crate::engine::Weavatrix;
use blazingly_json::{Value, json};
use std::time::{SystemTime, UNIX_EPOCH};
use weavatrix_parse::Language;

use super::transport_fixture::TempRepository;

mod extraction;
mod integration;
mod runtime;
