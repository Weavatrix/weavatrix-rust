#[cfg(feature = "vector")]
use crate::operations::{arg_bool, arg_u64};
use blazingly_json::Value;
#[cfg(feature = "vector")]
use blazingly_json::json;

#[cfg(feature = "vector")]
pub fn search(args: &Value) -> Result<Value, String> {
    use weavatrix_search_vector::{ExactIndex, IndexConfig, VectorIndex};

    let items = args
        .get("vectors")
        .and_then(Value::as_array)
        .ok_or_else(|| "vectors must be an array".to_owned())?;
    let mut labels = Vec::with_capacity(items.len());
    let mut vectors = Vec::with_capacity(items.len());
    for item in items {
        labels.push(
            item.get("node")
                .and_then(Value::as_str)
                .ok_or_else(|| "vector.node must be a string".to_owned())?
                .to_owned(),
        );
        vectors
            .push(values(item.get("values").ok_or_else(|| {
                "vector.values must be an array".to_owned()
            })?)?);
    }
    let query = values(
        args.get("query")
            .ok_or_else(|| "query must be an array".to_owned())?,
    )?;
    let dimensions = query.len();
    if dimensions == 0 || vectors.iter().any(|vector| vector.len() != dimensions) {
        return Err("query and every vector must have the same non-zero dimension".to_owned());
    }
    let borrowed = vectors
        .iter()
        .enumerate()
        .map(|(index, vector)| {
            (
                u64::try_from(index).map_err(|_| "vector index overflow"),
                vector.as_slice(),
            )
        })
        .map(|(key, vector)| key.map(|key| (key, vector)))
        .collect::<Result<Vec<_>, _>>()?;
    let top = usize::try_from(arg_u64(args, "top_k").unwrap_or(10)).unwrap_or(10);
    let exact = arg_bool(args, "exact").unwrap_or(items.len() < 2_000);
    let config = IndexConfig::new(dimensions);
    let hits = if exact {
        ExactIndex::build(config, &borrowed).and_then(|index| index.search(&query, top))
    } else {
        VectorIndex::build(config, &borrowed).and_then(|index| index.search(&query, top))
    }
    .map_err(|error| error.to_string())?;
    Ok(json!({
        "backend": if exact {"exact"} else {"hnsw"},
        "exact": exact,
        "dimensions": dimensions,
        "vectors": vectors.len(),
        "hits": hits.into_iter().filter_map(|hit| {
            let index = usize::try_from(hit.key).ok()?;
            Some(json!({"node": labels.get(index)?, "distance": hit.distance}))
        }).collect::<Vec<_>>()
    }))
}

#[cfg(not(feature = "vector"))]
pub fn search(_args: &Value) -> Result<Value, String> {
    Err("vector capability is not compiled".to_owned())
}

#[cfg(feature = "vector")]
fn values(value: &Value) -> Result<Vec<f32>, String> {
    super::vector_values(value, "vector values must be an array")
}
