use super::low_signal::LowSignalSuppression;
use blazingly_json::{Value, json};
use weavatrix_clone::{CloneFamily, CloneLocation, ClonePair, CloneStatistics};

#[allow(clippy::too_many_arguments)]
pub(super) fn report(
    statistics: &CloneStatistics,
    families: &[CloneFamily],
    pairs: &[ClonePair],
    top: usize,
    suppressed_families: usize,
    suppressed_pairs: usize,
    low_signal: LowSignalSuppression,
) -> Value {
    json!({
        "families": families.iter().take(top).map(|family| json!({
            "id": family.id,
            "members": family.members.iter().map(location).collect::<Vec<_>>(),
            "pairs": family.pair_ids
        })).collect::<Vec<_>>(),
        "pairs": pairs.iter().take(top).map(|pair| json!({
            "id": pair.id,
            "kind": format!("{:?}", pair.kind).to_ascii_lowercase(),
            "similarity_percent": pair.similarity.percent(),
            "left": location(&pair.left),
            "right": location(&pair.right),
            "evidence": {
                "strict_equal": pair.evidence.strict_equal,
                "renamed_equal": pair.evidence.renamed_equal,
                "edit_distance": pair.evidence.edit_distance,
                "compared_tokens": pair.evidence.compared_tokens
            }
        })).collect::<Vec<_>>(),
        "suppressed": {
            "families": suppressed_families,
            "pairs": suppressed_pairs,
            "boilerplate_families": low_signal.boilerplate,
            "declarative_families": low_signal.declarative,
            "detail": "test/classified evidence and router/handler boilerplate are suppressed by default; clone families are otherwise high-recall, while include_declarative=false explicitly suppresses data-only catalogs but keeps model/schema/contract evidence"
        },
        "statistics": {
            "source_files": statistics.source_files,
            "source_tokens": statistics.source_tokens,
            "candidate_pairs": statistics.candidate_pairs,
            "verified_pairs": statistics.verified_pairs
        }
    })
}

/// Line and byte extent of the matched region.
///
/// Both ranges are inclusive of whole lines only: the byte offsets bound
/// exactly the text the line range names, so a caller can compare two sites
/// byte for byte and reproduce the evidence.
fn location(location: &CloneLocation) -> Value {
    json!({
        "fragment_id": location.fragment_id,
        "path": location.path,
        "start_line": location.span.start_line,
        "end_line": location.span.end_line,
        "start_byte": location.span.start_byte,
        "end_byte": location.span.end_byte
    })
}
