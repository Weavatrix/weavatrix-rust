use crate::engine::RepositoryState;
#[cfg(feature = "semantic")]
use crate::operations::{arg_bool, arg_str, arg_u64};
use blazingly_json::Value;
#[cfg(feature = "semantic")]
use blazingly_json::json;

#[cfg(feature = "semantic")]
use weavatrix_semantic::{
    LinkConfig, SelectionMode, SemanticLinkReport, SemanticLinker, SemanticVector, SeoLinkPolicy,
    SeoPage, VectorCandidateConfig, VectorSemanticLinker,
};

#[cfg(feature = "semantic")]
pub fn semantic_link(state: &RepositoryState, args: &Value) -> Result<Value, String> {
    let vectors = vectors(args)?;
    let config = link_config(args, SelectionMode::Union);
    let report = link(state, &vectors, config, None)?;
    Ok(report_json(&report))
}

#[cfg(not(feature = "semantic"))]
pub fn semantic_link(_state: &RepositoryState, _args: &Value) -> Result<Value, String> {
    Err("semantic capability is not compiled".to_owned())
}

#[cfg(feature = "semantic")]
pub fn seo_links(state: &RepositoryState, args: &Value) -> Result<Value, String> {
    let vectors = vectors(args)?;
    let pages = args
        .get("pages")
        .and_then(Value::as_array)
        .ok_or_else(|| "pages must be an array".to_owned())?
        .iter()
        .map(page)
        .collect::<Result<Vec<_>, _>>()?;
    let policy = SeoLinkPolicy::new(pages)
        .map_err(|error| error.to_string())?
        .with_cross_language(arg_bool(args, "allow_cross_language").unwrap_or(false));
    let config = link_config(args, SelectionMode::Directed);
    let report = link(state, &vectors, config, Some(&policy))?;
    Ok(json!({
        "recommendations": report.edges(),
        "statistics": report_json(&report),
        "evidence": "INFERRED",
        "mutation": "NONE",
        "crawler": "CALLER_BOUNDARY"
    }))
}

#[cfg(not(feature = "semantic"))]
pub fn seo_links(_state: &RepositoryState, _args: &Value) -> Result<Value, String> {
    Err("semantic capability is not compiled".to_owned())
}

#[cfg(feature = "semantic")]
fn vectors(args: &Value) -> Result<Vec<SemanticVector>, String> {
    args.get("vectors")
        .and_then(Value::as_array)
        .ok_or_else(|| "vectors must be an array".to_owned())?
        .iter()
        .map(|item| {
            let node = item
                .get("node")
                .and_then(Value::as_str)
                .ok_or_else(|| "vector.node must be a string".to_owned())?;
            let values = item
                .get("values")
                .ok_or_else(|| "vector.values must be an array".to_owned())
                .and_then(|values| {
                    super::vector_values(values, "vector.values must be an array")
                })?;
            SemanticVector::new(node, values).map_err(|error| error.to_string())
        })
        .collect()
}

#[cfg(feature = "semantic")]
fn link_config(args: &Value, default_selection: SelectionMode) -> LinkConfig {
    let selection = match arg_str(args, "selection").ok() {
        Some("mutual") => SelectionMode::Mutual,
        Some("directed") => SelectionMode::Directed,
        Some("union") => SelectionMode::Union,
        _ => default_selection,
    };
    LinkConfig::new(
        arg_str(args, "model").unwrap_or("caller-supplied"),
        args.get("min_similarity")
            .and_then(Value::as_f64)
            .unwrap_or(0.78),
        usize::try_from(arg_u64(args, "top_k").unwrap_or(8)).unwrap_or(8),
    )
    .with_selection(selection)
}

#[cfg(feature = "semantic")]
fn link(
    state: &RepositoryState,
    vectors: &[SemanticVector],
    config: LinkConfig,
    policy: Option<&SeoLinkPolicy>,
) -> Result<SemanticLinkReport, String> {
    macro_rules! run {
        ($linker:expr) => {{
            let linker = $linker;
            apply_policy(
                policy,
                || {
                    linker
                        .link(state.graph(), vectors)
                        .map_err(|error| error.to_string())
                },
                |policy| {
                    linker
                        .link_with_policy(state.graph(), vectors, policy)
                        .map_err(|error| error.to_string())
                },
            )
        }};
    }

    let dimensions = vectors.first().map_or(0, SemanticVector::dimension);
    if vectors.len() >= 2_000 {
        let linker = VectorSemanticLinker::new(config, VectorCandidateConfig::new(dimensions))
            .map_err(|error| error.to_string())?;
        return run!(linker);
    }
    let linker = SemanticLinker::new(config).map_err(|error| error.to_string())?;
    run!(linker)
}

#[cfg(feature = "semantic")]
fn apply_policy(
    policy: Option<&SeoLinkPolicy>,
    without: impl FnOnce() -> Result<SemanticLinkReport, String>,
    with: impl FnOnce(&SeoLinkPolicy) -> Result<SemanticLinkReport, String>,
) -> Result<SemanticLinkReport, String> {
    match policy {
        Some(policy) => with(policy),
        None => without(),
    }
}

#[cfg(feature = "semantic")]
fn page(value: &Value) -> Result<SeoPage, String> {
    let node = value
        .get("node")
        .and_then(Value::as_str)
        .ok_or_else(|| "page.node must be a string".to_owned())?
        .parse()
        .map_err(|error: weavatrix_graph::GraphError| error.to_string())?;
    let mut page = SeoPage::new(
        node,
        value
            .get("site")
            .and_then(Value::as_str)
            .ok_or_else(|| "page.site must be a string".to_owned())?,
        value
            .get("canonical")
            .and_then(Value::as_str)
            .ok_or_else(|| "page.canonical must be a string".to_owned())?,
    )
    .map_err(|error| error.to_string())?;
    if let Some(language) = value.get("language").and_then(Value::as_str) {
        page = page
            .with_language(language)
            .map_err(|error| error.to_string())?;
    }
    page = page
        .with_source_eligible(
            value
                .get("source_eligible")
                .and_then(Value::as_bool)
                .unwrap_or(true),
        )
        .with_target_eligible(
            value
                .get("target_eligible")
                .and_then(Value::as_bool)
                .unwrap_or(true),
        )
        .with_cornerstone(
            value
                .get("cornerstone")
                .and_then(Value::as_bool)
                .unwrap_or(false),
        )
        .with_orphan(
            value
                .get("orphan")
                .and_then(Value::as_bool)
                .unwrap_or(false),
        )
        .with_target_priority(
            value
                .get("target_priority")
                .and_then(Value::as_u64)
                .and_then(|value| u32::try_from(value).ok())
                .unwrap_or(0),
        );
    for target in value
        .get("existing_targets")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
    {
        page = page.with_existing_target(
            target
                .parse()
                .map_err(|error: weavatrix_graph::GraphError| error.to_string())?,
        );
    }
    Ok(page)
}

#[cfg(feature = "semantic")]
fn report_json(report: &SemanticLinkReport) -> Value {
    json!({
        "vector_count": report.vector_count(),
        "dimension": report.dimension(),
        "comparisons": report.comparisons(),
        "pairs": report.pair_count(),
        "edges": report.edges(),
        "candidate_backend": report.candidate_backend().as_str(),
        "candidate_exact": report.candidate_backend().is_exact(),
        "policy": report.policy_id()
    })
}
