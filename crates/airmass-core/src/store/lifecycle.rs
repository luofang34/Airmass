//! Weather effective, validity, cancellation, and expiry rules.

use alloc::{collections::BTreeMap, vec::Vec};

use crate::{
    AbsenceEvidence, AbsenceReason, AbsentField, DeadlineSet, EvaluationTime, FieldState,
    MonotonicTime, StoreConfig, UtcTime, WeatherIngressEvent, WeatherProduct, WeatherProductState,
};

use super::{Candidate, ProductEntry, PublishedBody, SourceSlot, selection};

pub(super) fn is_future(candidate: &Candidate, now: UtcTime) -> bool {
    activation_time(&candidate.event).is_some_and(|activation| activation > now)
}

pub(super) fn promote_pending(
    products: &mut BTreeMap<crate::WeatherProductId, ProductEntry>,
    now: EvaluationTime,
) {
    for entry in products.values_mut() {
        for slot in entry.sources.values_mut() {
            promote_slot(slot, now.utc);
        }
    }
}

fn promote_slot(slot: &mut SourceSlot, now: UtcTime) {
    let becomes_current = slot
        .pending
        .as_ref()
        .is_some_and(|candidate| !is_future(candidate, now));
    if becomes_current {
        slot.current = slot.pending.take();
    }
}

pub(super) fn published_body(
    entry: &ProductEntry,
    config: &StoreConfig,
    now: EvaluationTime,
) -> Option<PublishedBody> {
    let candidates: Vec<&Candidate> = entry
        .sources
        .values()
        .filter_map(SourceSlot::visible_candidate)
        .collect();
    let current: Vec<&Candidate> = candidates
        .iter()
        .copied()
        .filter(|candidate| is_current(candidate, now))
        .collect();
    let eligible = if current.is_empty() {
        candidates.as_slice()
    } else {
        current.as_slice()
    };
    let selected = selection::select(eligible, config)?;
    Some(PublishedBody {
        source_product_revision: selected.candidate.source_product_revision,
        state: state(selected.candidate, now),
        selection: selected.explanation,
    })
}

fn is_current(candidate: &Candidate, now: EvaluationTime) -> bool {
    match &candidate.event {
        WeatherIngressEvent::Cancellation(_) => !is_future(candidate, now.utc),
        WeatherIngressEvent::Product(product) => {
            if product_activation_time(product).is_some_and(|value| now.utc < value) {
                return false;
            }
            let (utc_expiry, monotonic_expiry) = expiry_times(product);
            utc_expiry.is_none_or(|value| now.utc < value)
                && monotonic_expiry.is_none_or(|value| now.monotonic < value)
        }
    }
}

fn state(candidate: &Candidate, now: EvaluationTime) -> WeatherProductState {
    match &candidate.event {
        WeatherIngressEvent::Product(product) => product_state(product, now),
        WeatherIngressEvent::Cancellation(cancellation) => {
            if let Some(activation) = present_value(cancellation.effective_at())
                && now.utc < activation
            {
                return WeatherProductState::Absent {
                    absence: lifecycle_absence(
                        AbsenceReason::NotEffective,
                        Some(activation),
                        None,
                        cancellation.reason().provenance.clone(),
                    ),
                };
            }
            let reason = cancellation.reason();
            WeatherProductState::Cancelled {
                absence: AbsentField::new(
                    AbsenceReason::Cancelled,
                    AbsenceEvidence::Source {
                        time: reason.time,
                        quality: reason.quality.clone(),
                        provenance: reason.provenance.clone(),
                    },
                ),
                cancellation: cancellation.clone(),
            }
        }
    }
}

fn product_state(product: &WeatherProduct, now: EvaluationTime) -> WeatherProductState {
    if let Some(activation) = product_activation_time(product)
        && now.utc < activation
    {
        return WeatherProductState::Absent {
            absence: lifecycle_absence(
                AbsenceReason::NotEffective,
                Some(activation),
                None,
                product.payload().provenance.clone(),
            ),
        };
    }
    let (utc_expiry, monotonic_expiry) = expiry_times(product);
    let expired_utc = utc_expiry.filter(|expiry| now.utc >= *expiry);
    let expired_monotonic = monotonic_expiry.filter(|expiry| now.monotonic >= *expiry);
    if expired_utc.is_some() || expired_monotonic.is_some() {
        return WeatherProductState::Absent {
            absence: lifecycle_absence(
                AbsenceReason::Expired,
                expired_utc,
                expired_monotonic,
                product.payload().provenance.clone(),
            ),
        };
    }
    WeatherProductState::Available {
        product: alloc::boxed::Box::new(product.clone()),
    }
}

fn lifecycle_absence(
    reason: AbsenceReason,
    utc: Option<UtcTime>,
    monotonic: Option<MonotonicTime>,
    provenance: crate::ProductProvenance,
) -> AbsentField {
    AbsentField::new(
        reason,
        AbsenceEvidence::Lifecycle {
            utc,
            monotonic,
            provenance: Some(provenance),
        },
    )
}

fn activation_time(event: &WeatherIngressEvent) -> Option<UtcTime> {
    match event {
        WeatherIngressEvent::Product(product) => product_activation_time(product),
        WeatherIngressEvent::Cancellation(cancellation) => {
            present_value(cancellation.effective_at())
        }
    }
}

fn product_activation_time(product: &WeatherProduct) -> Option<UtcTime> {
    let effective = present_value(product.effective_at());
    let valid_start = present_value(product.valid_period()).map(|interval| interval.start);
    match (effective, valid_start) {
        (Some(left), Some(right)) => Some(left.max(right)),
        (Some(value), None) | (None, Some(value)) => Some(value),
        (None, None) => None,
    }
}

fn expiry_times(product: &WeatherProduct) -> (Option<UtcTime>, Option<MonotonicTime>) {
    let utc = present_value(product.valid_period()).map(|interval| interval.end);
    let monotonic = present_value(product.maximum_age()).and_then(|maximum_age| {
        product
            .payload()
            .time
            .ingress_monotonic
            .checked_add(maximum_age)
    });
    (utc, monotonic)
}

fn present_value<T: Copy>(field: &FieldState<T>) -> Option<T> {
    field.as_present().map(|present| present.value)
}

pub(super) fn next_deadline(
    products: &BTreeMap<crate::WeatherProductId, ProductEntry>,
    now: EvaluationTime,
) -> DeadlineSet {
    let mut deadlines = DeadlineSet::default();
    for entry in products.values() {
        for slot in entry.sources.values() {
            include_slot_deadlines(&mut deadlines, slot, now);
        }
    }
    deadlines
}

fn include_slot_deadlines(deadlines: &mut DeadlineSet, slot: &SourceSlot, now: EvaluationTime) {
    if let Some(candidate) = &slot.current {
        include_candidate_deadlines(deadlines, candidate, now);
    }
    if let Some(candidate) = &slot.pending {
        include_candidate_deadlines(deadlines, candidate, now);
    }
}

fn include_candidate_deadlines(
    deadlines: &mut DeadlineSet,
    candidate: &Candidate,
    now: EvaluationTime,
) {
    if let Some(activation) = activation_time(&candidate.event)
        && activation > now.utc
    {
        deadlines.include_utc(activation);
    }
    let WeatherIngressEvent::Product(product) = &candidate.event else {
        return;
    };
    let (utc_expiry, monotonic_expiry) = expiry_times(product);
    if let Some(expiry) = utc_expiry
        && expiry > now.utc
    {
        deadlines.include_utc(expiry);
    }
    if let Some(expiry) = monotonic_expiry
        && expiry > now.monotonic
    {
        deadlines.include_monotonic(expiry);
    }
}
