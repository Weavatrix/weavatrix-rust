use blazingly_json::Value;

pub(super) struct ContractResults {
    pub(super) transport: String,
    pub(super) http: Value,
    pub(super) events: Value,
    pub(super) graphql: Value,
    pub(super) grpc: Value,
}

pub(super) struct EvidencePage {
    pub(super) detail: String,
    pub(super) offset: usize,
    pub(super) page_size: usize,
    pub(super) total_items: usize,
    pub(super) end: usize,
    pub(super) items: Vec<Value>,
}

pub(super) struct TraceSummary {
    pub(super) verdict: &'static str,
    pub(super) http_mismatches: u64,
    pub(super) event_mismatches: u64,
    pub(super) typed_mismatches: u64,
    pub(super) matched: u64,
}
