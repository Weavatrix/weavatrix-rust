//! Web3 ABI, compiler artifacts, and static viem/wagmi consumers.
//!
//! Read-only. Does not compile contracts, call RPC, or execute repository code.

mod abi;
mod artifacts;
mod clients;
mod detect;
mod facts;
mod hash;
mod identity;
mod kinds;
mod limits;
mod span;

pub(crate) use abi::silent_misdecode_example;
pub(crate) use detect::{admitted_size, looks_promising};
pub(crate) use kinds::binds as web3_binds;
pub(crate) use limits::MAX_BUILD_INFO_BYTES;

use crate::language::FileFacts;
use blazingly_json::Value;

#[must_use]
pub(crate) fn analyze(path: &str, raw: &str, value: &Value) -> Option<FileFacts> {
    let class = detect::classify(value);
    if class == detect::DocumentClass::Other {
        return None;
    }
    let mut diagnostics = Vec::new();
    if class == detect::DocumentClass::SolcInput {
        let _ = abi::Profile::SolcInput;
        diagnostics.push(crate::model::Diagnostic {
            code: "web3.artifact.input_only".into(),
            message:
                "solc standard JSON input was recorded; ABI members require a supplied output pair"
                    .into(),
            span: Some(span::file_span(path)),
        });
        return Some(FileFacts {
            diagnostics,
            ..FileFacts::default()
        });
    }
    let documents = artifacts::extract(path, raw, value, class);
    if documents.is_empty() && diagnostics.is_empty() && class != detect::DocumentClass::SolcInput {
        return None;
    }
    Some(facts::documents_to_facts(path, &documents, diagnostics))
}

pub(crate) fn overlay_clients(path: &str, raw: &str, facts: &mut FileFacts) {
    let occurrences = clients::extract(path, raw, &facts.imports);
    if occurrences.is_empty() {
        return;
    }
    facts::emit_consumers(facts, path, &occurrences);
}

#[cfg(test)]
mod tests {
    use super::overlay_clients;
    use crate::language::FileFacts;

    #[test]
    fn viem_decode_event_log_is_a_consumer() {
        let mut facts = FileFacts::default();
        overlay_clients(
            "frontend/src/events.ts",
            include_str!("../../../tests/fixtures/web3/events.ts"),
            &mut facts,
        );
        assert!(
            facts
                .symbols
                .iter()
                .filter(|symbol| symbol.kind.as_str() == "web3.consumer")
                .count()
                >= 2
        );
        assert!(
            facts
                .domains
                .iter()
                .any(|domain| domain.name == "web3.unresolved:dynamic_or_spread")
        );
    }
}
