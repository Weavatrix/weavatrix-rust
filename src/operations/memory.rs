use crate::engine::RepositoryState;
use blazingly_json::Value;
#[cfg(feature = "memory")]
use blazingly_json::json;

#[cfg(feature = "memory")]
pub fn context(_state: &RepositoryState, args: &Value) -> Result<Value, String> {
    use weavatrix_memory::{
        BytesTokenEstimator, ContextCompiler, ContextRequest, MemoryEvent, MemoryProjection,
        StoredEvent, replay_owned,
    };

    let events = blazingly_json::from_value::<Vec<StoredEvent<MemoryEvent>>>(
        args.get("events")
            .cloned()
            .ok_or_else(|| "events must be an array".to_owned())?,
    )
    .map_err(|error| format!("invalid memory events: {error}"))?;
    let projection: MemoryProjection = replay_owned(events).map_err(|error| error.to_string())?;
    let request = blazingly_json::from_value::<ContextRequest>(
        args.get("request")
            .cloned()
            .ok_or_else(|| "request must contain a ContextRequest object".to_owned())?,
    )
    .map_err(|error| format!("invalid memory request: {error}"))?;
    let bundle = ContextCompiler::new(BytesTokenEstimator::default())
        .compile(&projection, &request)
        .map_err(|error| error.to_string())?;
    Ok(json!({
        "view": bundle.view,
        "graph": bundle.graph,
        "receipt": bundle.receipt,
        "mutation": "NONE"
    }))
}

#[cfg(not(feature = "memory"))]
pub fn context(_state: &RepositoryState, _args: &Value) -> Result<Value, String> {
    Err("memory capability is not compiled".to_owned())
}
