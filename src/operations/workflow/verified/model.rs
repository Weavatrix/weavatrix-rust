use blazingly_json::Value;
pub(super) struct ContextEvidence {
    pub(super) retrieval: Value,
    pub(super) edit_contexts: Vec<Value>,
    pub(super) data_flow: Value,
}

pub(super) struct VerificationChecks {
    pub(super) graph_baseline: Value,
    pub(super) architecture: Value,
    pub(super) audit: Value,
    pub(super) duplicates: Value,
    pub(super) api_contract: Value,
}

pub(super) struct TestEvidence {
    pub(super) suggested: Vec<String>,
    pub(super) requested: Vec<String>,
    pub(super) value: Value,
}

pub(super) struct Assessment {
    pub(super) verdict: &'static str,
    pub(super) blockers: Vec<String>,
    pub(super) limitations: Vec<String>,
}
