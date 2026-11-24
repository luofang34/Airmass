use alloc::vec;

use super::super::*;
use super::{absent, at, current_product, field, product_ingress};
use crate::{DurationMillis, FieldState, MonotonicTime, WeatherPayload, WeatherProduct};

#[test]
fn snapshot_revision_exhaustion_ends_the_producer() {
    let mut store = WeatherStore::new(StoreConfig::default(), ProducerInstanceId::new(1))
        .expect("configuration must be valid");
    store
        .accept(product_ingress("alpha", 1, 1, 1, 10, None), at(10, 10))
        .expect("first revision must be accepted");
    store.snapshot_revision = SnapshotRevision::new(u64::MAX);
    let error = store
        .accept(product_ingress("alpha", 1, 2, 2, 20, None), at(20, 20))
        .expect_err("the snapshot revision must not wrap");
    assert!(matches!(
        error,
        StoreError::SnapshotRevisionExhausted { .. }
    ));
    assert_eq!(
        store.capture().snapshot_revision(),
        SnapshotRevision::new(1)
    );
    let ended = store
        .advance_time(at(30, 30))
        .expect_err("producer must end after revision exhaustion");
    assert!(matches!(ended, StoreError::ProducerEnded { .. }));
}

#[test]
fn monotonic_expiry_deadline_must_fit_the_clock_range() {
    let monotonic = u64::MAX;
    let product = WeatherProduct::new(
        field(
            WeatherPayload::new("application/octet-stream", vec![1]),
            "alpha",
            1,
            monotonic,
        ),
        absent(),
        absent(),
        absent(),
        absent(),
        FieldState::present(field(DurationMillis::new(1), "alpha", 1, monotonic)),
    );
    let ingress = WeatherIngress::product(
        WeatherProductId::new("regional-radar"),
        SourceProductRevision::new(1),
        product,
    );
    let mut store = WeatherStore::new(StoreConfig::default(), ProducerInstanceId::new(1))
        .expect("configuration must be valid");
    let error = store
        .accept(
            ingress,
            EvaluationTime::new(
                crate::UtcTime::from_unix_millis(1),
                MonotonicTime::from_micros(monotonic),
            ),
        )
        .expect_err("expiry overflow must fail");
    assert!(matches!(error, StoreError::MonotonicExpiryOverflow { .. }));
    assert!(store.capture().snapshot().products().is_empty());
}

#[test]
fn product_and_snapshot_revision_types_are_not_interchangeable() {
    let mut store = WeatherStore::new(StoreConfig::default(), ProducerInstanceId::new(1))
        .expect("configuration must be valid");
    store
        .accept(product_ingress("alpha", 1, 1, 1, 10, None), at(10, 10))
        .expect("first revision must be accepted");
    let second_product = match product_ingress("alpha", 1, 1, 1, 20, None)
        .into_event()
        .expect("fixture schema must be current")
    {
        WeatherIngressEvent::Product(product) => *product,
        WeatherIngressEvent::Cancellation(_) => panic!("fixture must be a product"),
    };
    store
        .accept(
            WeatherIngress::product(
                WeatherProductId::new("winds"),
                SourceProductRevision::new(1),
                second_product,
            ),
            at(20, 20),
        )
        .expect("second product must be accepted");
    store
        .accept(product_ingress("alpha", 1, 2, 2, 30, None), at(30, 30))
        .expect("second revision must be accepted");
    let product_revision = current_product(&store).product_revision();
    let snapshot_revision = store.capture().snapshot_revision();
    assert_eq!(product_revision.get(), 2);
    assert_eq!(snapshot_revision.get(), 3);
}
