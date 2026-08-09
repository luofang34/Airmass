//! Deterministic weather source comparison.

use alloc::vec::Vec;
use core::cmp::Ordering;

use crate::{SelectionCriterion, SelectionExplanation, StoreConfig};

use super::Candidate;

pub(super) struct Selected<'a> {
    pub(super) candidate: &'a Candidate,
    pub(super) explanation: SelectionExplanation,
}

pub(super) fn select<'a>(
    candidates: &'a [&'a Candidate],
    config: &StoreConfig,
) -> Option<Selected<'a>> {
    let mut ordered: Vec<&Candidate> = candidates.to_vec();
    ordered.sort_by(|left, right| compare(right, left, config).0);
    let selected = *ordered.first()?;
    let criterion = ordered
        .get(1)
        .map_or(SelectionCriterion::SoleSource, |runner_up| {
            compare(selected, runner_up, config).1
        });
    let considered_sources = u16::try_from(ordered.len()).ok()?;
    Some(Selected {
        candidate: selected,
        explanation: SelectionExplanation {
            selected_source: selected.source().clone(),
            considered_sources,
            decisive_criterion: criterion,
        },
    })
}

fn compare(
    left: &Candidate,
    right: &Candidate,
    config: &StoreConfig,
) -> (Ordering, SelectionCriterion) {
    let left_rank = config.source_rank(&left.source().id);
    let right_rank = config.source_rank(&right.source().id);
    if left_rank != right_rank {
        return (
            right_rank.cmp(&left_rank),
            SelectionCriterion::ConfiguredPriority,
        );
    }
    if let Some(ordering) = compare_source_time(left, right)
        && ordering != Ordering::Equal
    {
        return (ordering, SelectionCriterion::KnownSourceTime);
    }
    let ingress_order = left
        .event
        .anchor_time()
        .ingress_monotonic
        .cmp(&right.event.anchor_time().ingress_monotonic);
    if ingress_order != Ordering::Equal {
        return (ingress_order, SelectionCriterion::IngressOrder);
    }
    (
        right.source().cmp(left.source()),
        SelectionCriterion::StableSourceIdentity,
    )
}

fn compare_source_time(left: &Candidate, right: &Candidate) -> Option<Ordering> {
    match (left.comparison_time(), right.comparison_time()) {
        (Some((left_value, left_kind)), Some((right_value, right_kind)))
            if left_kind == right_kind =>
        {
            Some(left_value.cmp(&right_value))
        }
        (Some(_), None) => Some(Ordering::Greater),
        (None, Some(_)) => Some(Ordering::Less),
        _ => None,
    }
}
