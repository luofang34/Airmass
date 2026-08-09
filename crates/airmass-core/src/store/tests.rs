#![allow(clippy::expect_used, clippy::panic)]

use alloc::vec;

use super::*;
use crate::{
    AbsenceEvidence, AbsenceReason, AbsentField, Cancellation, CancellationReason, DeliveryPath,
    DurationMillis, FieldState, MonotonicTime, ProductProvenance, QualityEvidence,
    RawProductIdentity, SelectionCriterion, SourceTime, SourceTimeKind, TimeBasis, TimeEvidence,
    TimeQuality, TimeUncertainty, TimedField, UtcInterval, UtcTime, WeatherOrigin, WeatherPayload,
    WeatherProduct, WeatherProductState,
};

mod counters;

const PRODUCT: &str = "regional-radar";

pub(super) fn at(utc: i64, monotonic: u64) -> EvaluationTime {
    EvaluationTime::new(UtcTime::from_unix_millis(utc), monotonic_millis(monotonic))
}

fn monotonic_millis(value: u64) -> MonotonicTime {
    MonotonicTime::from_micros(value.saturating_mul(1_000))
}

fn provenance(source: &str, epoch: u64) -> ProductProvenance {
    ProductProvenance::new(
        SourceRef::new(SourceId::new(source), SourceEpoch::new(epoch)),
        WeatherOrigin::Simulation,
        DeliveryPath::new("test"),
        Some(RawProductIdentity::new("test", PRODUCT)),
    )
}

fn time(monotonic: u64, source_time: Option<i64>) -> TimeEvidence {
    let source = source_time.map_or(SourceTime::Unknown, |millis| SourceTime::Known {
        value: UtcTime::from_unix_millis(millis),
        kind: SourceTimeKind::Product,
        basis: TimeBasis::SourceUtc,
        quality: TimeQuality::Exact,
        uncertainty: TimeUncertainty::Bounded(0),
    });
    TimeEvidence::new(monotonic_millis(monotonic), source)
}

pub(super) fn field<T>(value: T, source: &str, epoch: u64, monotonic: u64) -> TimedField<T> {
    TimedField::new(
        value,
        time(monotonic, None),
        QualityEvidence::Unavailable,
        provenance(source, epoch),
    )
}

pub(super) fn absent<T>() -> FieldState<T> {
    FieldState::absent(AbsentField::new(
        AbsenceReason::NotObserved,
        AbsenceEvidence::Unknown,
    ))
}

pub(super) fn product_ingress(
    source: &str,
    epoch: u64,
    revision: u64,
    payload: u8,
    monotonic: u64,
    source_time: Option<i64>,
) -> WeatherIngress {
    let payload = TimedField::new(
        WeatherPayload::new("application/octet-stream", vec![payload]),
        time(monotonic, source_time),
        QualityEvidence::Unavailable,
        provenance(source, epoch),
    );
    WeatherIngress::product(
        WeatherProductId::new(PRODUCT),
        SourceProductRevision::new(revision),
        WeatherProduct::new(payload, absent(), absent(), absent(), absent()),
    )
}

fn expiring_ingress(valid_end: i64, max_age: u64) -> WeatherIngress {
    let source = "alpha";
    let epoch = 1;
    let monotonic = 10;
    let product = WeatherProduct::new(
        field(
            WeatherPayload::new("application/octet-stream", vec![1]),
            source,
            epoch,
            monotonic,
        ),
        absent(),
        absent(),
        FieldState::present(field(
            UtcInterval::new(
                UtcTime::from_unix_millis(0),
                UtcTime::from_unix_millis(valid_end),
            ),
            source,
            epoch,
            monotonic,
        )),
        FieldState::present(field(
            DurationMillis::new(max_age),
            source,
            epoch,
            monotonic,
        )),
    );
    WeatherIngress::product(
        WeatherProductId::new(PRODUCT),
        SourceProductRevision::new(1),
        product,
    )
}

fn cancellation_ingress(revision: u64, monotonic: u64) -> WeatherIngress {
    WeatherIngress::cancellation(
        WeatherProductId::new(PRODUCT),
        SourceProductRevision::new(revision),
        Cancellation::new(
            field(CancellationReason::Withdrawn, "alpha", 1, monotonic),
            absent(),
        ),
    )
}

pub(super) fn current_product(store: &WeatherStore) -> &WeatherProductSnapshot {
    store
        .current
        .snapshot()
        .product(&WeatherProductId::new(PRODUCT))
        .expect("test product must exist")
}

#[test]
fn product_and_snapshot_revisions_advance_separately() {
    let mut store = WeatherStore::new(StoreConfig::default(), ProducerInstanceId::new(7))
        .expect("configuration must be valid");
    let first = store
        .accept(
            product_ingress("alpha", 1, 1, 1, 10, Some(100)),
            at(100, 10),
        )
        .expect("first revision must be accepted");
    assert_eq!(first.status(), AcceptStatus::Applied);
    assert_eq!(
        current_product(&store).product_revision(),
        ProductRevision::new(1)
    );
    assert_eq!(
        store.capture().snapshot_revision(),
        SnapshotRevision::new(1)
    );

    store
        .accept(
            product_ingress("alpha", 1, 2, 2, 20, Some(200)),
            at(200, 20),
        )
        .expect("second revision must be accepted");
    assert_eq!(
        current_product(&store).product_revision(),
        ProductRevision::new(2)
    );
    assert_eq!(
        store.capture().snapshot_revision(),
        SnapshotRevision::new(2)
    );
}

#[test]
fn replay_is_idempotent_and_conflicting_replay_is_an_error() {
    let mut store = WeatherStore::new(StoreConfig::default(), ProducerInstanceId::new(1))
        .expect("configuration must be valid");
    let ingress = product_ingress("alpha", 1, 4, 1, 10, None);
    store
        .accept(ingress.clone(), at(10, 10))
        .expect("first input must be accepted");
    let replay = store
        .accept(ingress, at(10, 10))
        .expect("exact replay must be accepted");
    assert_eq!(replay.status(), AcceptStatus::Replay);
    assert!(replay.publication().is_none());

    let error = store
        .accept(product_ingress("alpha", 1, 4, 2, 10, None), at(10, 10))
        .expect_err("different data at one revision must fail");
    assert!(matches!(error, StoreError::ConflictingReplay { .. }));
}

#[test]
fn cancellation_replaces_the_available_value_with_an_explicit_absence() {
    let mut store = WeatherStore::new(StoreConfig::default(), ProducerInstanceId::new(1))
        .expect("configuration must be valid");
    store
        .accept(product_ingress("alpha", 1, 1, 1, 10, None), at(10, 10))
        .expect("product must be accepted");
    store
        .accept(cancellation_ingress(2, 20), at(20, 20))
        .expect("cancellation must be accepted");
    let product = current_product(&store);
    assert_eq!(product.product_revision(), ProductRevision::new(2));
    assert!(matches!(
        product.state(),
        WeatherProductState::Cancelled { .. }
    ));
    assert_eq!(
        product.state().absence().map(AbsentField::reason),
        Some(AbsenceReason::Cancelled)
    );
}

#[test]
fn civil_and_monotonic_expiry_return_exact_deadlines() {
    let mut store = WeatherStore::new(StoreConfig::default(), ProducerInstanceId::new(1))
        .expect("configuration must be valid");
    let accepted = store
        .accept(expiring_ingress(100, 50), at(10, 10))
        .expect("product must be accepted");
    assert_eq!(
        accepted.next_deadline().utc,
        Some(UtcTime::from_unix_millis(100))
    );
    assert_eq!(
        accepted.next_deadline().monotonic,
        Some(monotonic_millis(60))
    );
    store
        .advance_time(at(60, 60))
        .expect("monotonic expiry must publish");
    assert_eq!(
        current_product(&store)
            .state()
            .absence()
            .map(AbsentField::reason),
        Some(AbsenceReason::Expired)
    );
    let unchanged = store
        .advance_time(at(70, 70))
        .expect("later time must be valid");
    assert!(unchanged.publication().is_none());
}

#[test]
fn configured_priority_is_deterministic_and_explained() {
    let config = StoreConfig {
        source_priorities: vec![
            SourcePriority::new(SourceId::new("alpha"), 1),
            SourcePriority::new(SourceId::new("beta"), 2),
        ],
        ..StoreConfig::default()
    };
    let mut store =
        WeatherStore::new(config, ProducerInstanceId::new(1)).expect("configuration must be valid");
    store
        .accept(product_ingress("alpha", 1, 1, 1, 10, Some(10)), at(20, 10))
        .expect("alpha must be accepted");
    store
        .accept(product_ingress("beta", 1, 1, 2, 20, Some(20)), at(30, 20))
        .expect("beta must be accepted");
    let selected = current_product(&store);
    assert_eq!(selected.selection().selected_source.id.as_str(), "alpha");
    assert_eq!(
        selected.selection().decisive_criterion,
        SelectionCriterion::ConfiguredPriority
    );
}

#[test]
fn expired_preferred_source_yields_to_a_current_source() {
    let config = StoreConfig {
        source_priorities: vec![
            SourcePriority::new(SourceId::new("alpha"), 1),
            SourcePriority::new(SourceId::new("beta"), 2),
        ],
        ..StoreConfig::default()
    };
    let mut store =
        WeatherStore::new(config, ProducerInstanceId::new(1)).expect("configuration must be valid");
    store
        .accept(expiring_ingress(30, 1_000), at(10, 10))
        .expect("preferred source must be accepted");
    store
        .accept(product_ingress("beta", 1, 1, 2, 20, Some(20)), at(20, 20))
        .expect("fallback source must be accepted");
    assert_eq!(
        current_product(&store)
            .selection()
            .selected_source
            .id
            .as_str(),
        "alpha"
    );
    store
        .advance_time(at(30, 30))
        .expect("validity deadline must publish fallback source");
    assert_eq!(
        current_product(&store)
            .selection()
            .selected_source
            .id
            .as_str(),
        "beta"
    );
    assert!(current_product(&store).state().available().is_some());
}

#[test]
fn known_source_time_wins_over_unknown_time() {
    let mut store = WeatherStore::new(StoreConfig::default(), ProducerInstanceId::new(1))
        .expect("configuration must be valid");
    store
        .accept(product_ingress("alpha", 1, 1, 1, 20, None), at(30, 20))
        .expect("unknown-time product must be accepted");
    store
        .accept(product_ingress("beta", 1, 1, 2, 10, Some(10)), at(30, 20))
        .expect("known-time product must be accepted");
    let selected = current_product(&store);
    assert_eq!(selected.selection().selected_source.id.as_str(), "beta");
    assert_eq!(
        selected.selection().decisive_criterion,
        SelectionCriterion::KnownSourceTime
    );
}

#[test]
fn a_new_source_epoch_resets_the_source_revision_sequence() {
    let mut store = WeatherStore::new(StoreConfig::default(), ProducerInstanceId::new(1))
        .expect("configuration must be valid");
    store
        .accept(product_ingress("alpha", 1, 9, 1, 10, None), at(10, 10))
        .expect("first source epoch must be accepted");
    store
        .accept(product_ingress("alpha", 2, 1, 2, 20, None), at(20, 20))
        .expect("new source epoch must be accepted");
    assert_eq!(
        current_product(&store).selection().selected_source.epoch,
        SourceEpoch::new(2)
    );
    let stale = store
        .accept(product_ingress("alpha", 1, 10, 3, 30, None), at(30, 30))
        .expect("old source epoch must be ignored");
    assert_eq!(stale.status(), AcceptStatus::Stale);
}

#[test]
fn future_effective_product_is_absent_until_its_deadline() {
    let product = match product_ingress("alpha", 1, 1, 1, 10, None)
        .into_event()
        .expect("fixture schema must be current")
    {
        WeatherIngressEvent::Product(product) => *product,
        WeatherIngressEvent::Cancellation(_) => panic!("fixture must be a product"),
    };
    let product = WeatherProduct::new(
        product.payload().clone(),
        product.issue_time().clone(),
        FieldState::present(field(UtcTime::from_unix_millis(50), "alpha", 1, 10)),
        product.valid_period().clone(),
        product.maximum_age().clone(),
    );
    let ingress = WeatherIngress::product(
        WeatherProductId::new(PRODUCT),
        SourceProductRevision::new(1),
        product,
    );
    let mut store = WeatherStore::new(StoreConfig::default(), ProducerInstanceId::new(1))
        .expect("configuration must be valid");
    let accepted = store
        .accept(ingress, at(20, 10))
        .expect("product must be accepted");
    assert_eq!(
        accepted.next_deadline().utc,
        Some(UtcTime::from_unix_millis(50))
    );
    assert_eq!(
        current_product(&store)
            .state()
            .absence()
            .map(AbsentField::reason),
        Some(AbsenceReason::NotEffective)
    );
    store
        .advance_time(at(50, 40))
        .expect("deadline must activate product");
    assert!(current_product(&store).state().available().is_some());
}

#[test]
fn retained_handle_does_not_change_after_publication() {
    let mut store = WeatherStore::new(StoreConfig::default(), ProducerInstanceId::new(1))
        .expect("configuration must be valid");
    store
        .accept(product_ingress("alpha", 1, 1, 1, 10, None), at(10, 10))
        .expect("first revision must be accepted");
    let retained = store.capture();
    store
        .accept(product_ingress("alpha", 1, 2, 2, 20, None), at(20, 20))
        .expect("second revision must be accepted");
    assert_eq!(retained.snapshot_revision(), SnapshotRevision::new(1));
    assert_eq!(
        store.capture().snapshot_revision(),
        SnapshotRevision::new(2)
    );
}

#[test]
fn wrapping_revision_ends_the_producer_without_publication() {
    let mut store = WeatherStore::new(StoreConfig::default(), ProducerInstanceId::new(1))
        .expect("configuration must be valid");
    store
        .accept(product_ingress("alpha", 1, 1, 1, 10, None), at(10, 10))
        .expect("first revision must be accepted");
    let entry = store
        .products
        .get_mut(&WeatherProductId::new(PRODUCT))
        .expect("product state must exist");
    entry.product_revision = ProductRevision::new(u64::MAX);
    let error = store
        .accept(product_ingress("alpha", 1, 2, 2, 20, None), at(20, 20))
        .expect_err("the product revision must not wrap");
    assert!(matches!(error, StoreError::ProductRevisionExhausted { .. }));
    let ended = store
        .advance_time(at(30, 30))
        .expect_err("producer must end after revision exhaustion");
    assert!(matches!(ended, StoreError::ProducerEnded { .. }));
}

#[test]
fn field_evidence_must_name_the_event_source() {
    let payload = field(
        WeatherPayload::new("application/octet-stream", vec![1]),
        "alpha",
        1,
        10,
    );
    let product = WeatherProduct::new(
        payload,
        FieldState::present(field(UtcTime::from_unix_millis(1), "beta", 1, 10)),
        absent(),
        absent(),
        absent(),
    );
    let ingress = WeatherIngress::product(
        WeatherProductId::new(PRODUCT),
        SourceProductRevision::new(1),
        product,
    );
    let mut store = WeatherStore::new(StoreConfig::default(), ProducerInstanceId::new(1))
        .expect("configuration must be valid");
    let error = store
        .accept(ingress, at(10, 10))
        .expect_err("mixed field sources must fail");
    assert!(matches!(error, StoreError::FieldSourceMismatch { .. }));
}
