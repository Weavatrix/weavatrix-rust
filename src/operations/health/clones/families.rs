//! Clone-family rebuilding after visibility and result-limit filtering.

use std::collections::BTreeSet;
use weavatrix_clone::{CloneFamily, ClonePair, families_for_pairs};

pub(super) fn rebuild(pairs: &[ClonePair]) -> Vec<CloneFamily> {
    let mut families = families_for_pairs(pairs);
    sort_by_size(&mut families);
    families
}

pub(super) fn limit(
    mut families: Vec<CloneFamily>,
    mut pairs: Vec<ClonePair>,
    top: usize,
) -> (Vec<CloneFamily>, Vec<ClonePair>) {
    families.truncate(top);
    let selected_pair_ids = families
        .iter()
        .flat_map(|family| family.pair_ids.iter().cloned())
        .collect::<BTreeSet<_>>();
    pairs.retain(|pair| selected_pair_ids.contains(&pair.id));
    pairs.truncate(top);
    (rebuild(&pairs), pairs)
}

fn sort_by_size(families: &mut [CloneFamily]) {
    families.sort_by_key(|family| {
        core::cmp::Reverse(
            family
                .members
                .iter()
                .map(|member| {
                    usize::try_from(member.span.end_line.saturating_sub(member.span.start_line))
                        .unwrap_or(0)
                        + 1
                })
                .sum::<usize>(),
        )
    });
}
