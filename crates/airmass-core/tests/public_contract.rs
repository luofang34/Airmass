//! Public tests for the versioned weather snapshot contract.

#![allow(clippy::expect_used, clippy::panic)]

use airmass_core::{
    AbsenceEvidence, AbsenceReason, AbsentField, CURRENT_WEATHER_INGRESS_SCHEMA_VERSION,
    CURRENT_WEATHER_SNAPSHOT_SCHEMA_VERSION, DeliveryPath, EvaluationTime, FieldState,
    MonotonicTime, ProducerInstanceId, ProductProvenance, QualityEvidence, RawProductIdentity,
    SnapshotRevision, SourceEpoch, SourceId, SourceProductRevision, SourceRef, SourceTime,
    StoreConfig, TimeEvidence, TimedField, UtcTime, WeatherIngress, WeatherIngressEvent,
    WeatherIngressHeader, WeatherOrigin, WeatherPayload, WeatherProduct, WeatherProductId,
    WeatherSchemaError, WeatherSnapshotRecord, WeatherSnapshotRecordHeader, WeatherStore,
};

fn absent<T>() -> FieldState<T> {
    FieldState::absent(AbsentField::new(
        AbsenceReason::NotObserved,
        AbsenceEvidence::Unknown,
    ))
}

fn ingress(revision: u64, byte: u8, monotonic: u64) -> WeatherIngress {
    let provenance = ProductProvenance::new(
        SourceRef::new(SourceId::new("network-a"), SourceEpoch::new(4)),
        WeatherOrigin::NetworkProvider,
        DeliveryPath::new("provider-api"),
        Some(RawProductIdentity::new("provider", "radar-1")),
    );
    let payload = TimedField::new(
        WeatherPayload::new("application/octet-stream", vec![byte]),
        TimeEvidence::new(MonotonicTime::from_millis(monotonic), SourceTime::Unknown),
        QualityEvidence::Unavailable,
        provenance,
    );
    WeatherIngress::product(
        WeatherProductId::new("radar"),
        SourceProductRevision::new(revision),
        WeatherProduct::new(payload, absent(), absent(), absent(), absent()),
    )
}

fn at(value: u64) -> EvaluationTime {
    EvaluationTime::new(
        UtcTime::from_unix_millis(value as i64),
        MonotonicTime::from_millis(value),
    )
}

#[test]
fn ingress_header_rejects_a_version_before_payload_decode() {
    let text = r#"{
        "schema_version": 77,
        "event": {"payload_that_this_reader_cannot_decode": true}
    }"#;
    let header: WeatherIngressHeader =
        serde_json::from_str(text).expect("header must ignore payload fields");
    assert_eq!(header.schema_version(), 77);
    assert_eq!(
        header.validate(),
        Err(WeatherSchemaError::UnsupportedIngressVersion {
            found: 77,
            supported: CURRENT_WEATHER_INGRESS_SCHEMA_VERSION,
        })
    );
}

#[test]
fn snapshot_header_rejects_a_version_before_envelope_decode() {
    let text = r#"{
        "schema_version": 88,
        "envelope": {"payload_that_this_reader_cannot_decode": true}
    }"#;
    let header: WeatherSnapshotRecordHeader =
        serde_json::from_str(text).expect("header must ignore envelope fields");
    assert_eq!(header.schema_version(), 88);
    assert_eq!(
        header.validate(),
        Err(WeatherSchemaError::UnsupportedSnapshotVersion {
            found: 88,
            supported: CURRENT_WEATHER_SNAPSHOT_SCHEMA_VERSION,
        })
    );
}

#[test]
fn snapshot_record_covers_the_complete_envelope() {
    let mut store = WeatherStore::new(StoreConfig::default(), ProducerInstanceId::new(19))
        .expect("configuration must be valid");
    store
        .accept(ingress(1, 4, 10), at(10))
        .expect("input must publish");
    let record = WeatherSnapshotRecord::new(store.capture().into_envelope());
    let encoded = serde_json::to_string(&record).expect("record must serialize");
    let header: WeatherSnapshotRecordHeader =
        serde_json::from_str(&encoded).expect("record header must deserialize");
    header
        .validate()
        .expect("current version must be supported");
    let decoded: WeatherSnapshotRecord =
        serde_json::from_str(&encoded).expect("record must deserialize");
    assert_eq!(decoded, record);
    assert_eq!(
        decoded
            .envelope()
            .expect("record version must be supported")
            .snapshot_revision(),
        SnapshotRevision::new(1)
    );
}

#[test]
fn retained_public_handle_stays_immutable() {
    let mut store = WeatherStore::new(StoreConfig::default(), ProducerInstanceId::new(5))
        .expect("configuration must be valid");
    store
        .accept(ingress(1, 1, 10), at(10))
        .expect("first input must publish");
    let retained = store.capture();
    store
        .accept(ingress(2, 2, 20), at(20))
        .expect("second input must publish");
    assert_eq!(retained.snapshot_revision(), SnapshotRevision::new(1));
    assert_eq!(
        store.capture().snapshot_revision(),
        SnapshotRevision::new(2)
    );
    let retained_payload = retained
        .snapshot()
        .product(&WeatherProductId::new("radar"))
        .and_then(|product| product.state().available())
        .map(|product| product.payload().value.bytes.as_slice());
    assert_eq!(retained_payload, Some(&[1][..]));
}

#[test]
fn serialized_fields_keep_evidence_and_explicit_absence() {
    let encoded = serde_json::to_value(ingress(1, 4, 10)).expect("ingress must serialize");
    let payload = &encoded["event"]["value"]["payload"];
    assert!(payload.get("value").is_some());
    assert!(payload.get("time").is_some());
    assert!(payload.get("quality").is_some());
    assert!(payload.get("provenance").is_some());
    assert_eq!(
        encoded["event"]["value"]["issue_time"]["field"]["reason"],
        "not_observed"
    );
}

#[test]
fn store_keeps_time_and_provenance_for_each_field() {
    let product = match ingress(1, 4, 10)
        .into_event()
        .expect("fixture schema must be current")
    {
        WeatherIngressEvent::Product(product) => *product,
        WeatherIngressEvent::Cancellation(_) => panic!("fixture must be a product"),
    };
    let issue_time = TimedField::new(
        UtcTime::from_unix_millis(2),
        TimeEvidence::new(MonotonicTime::from_millis(5), SourceTime::Unknown),
        QualityEvidence::Reported {
            scheme: "provider-quality".into(),
            value: "reviewed".into(),
        },
        product.payload().provenance.clone(),
    );
    let product = WeatherProduct::new(
        product.payload().clone(),
        FieldState::present(issue_time),
        product.effective_at().clone(),
        product.valid_period().clone(),
        product.maximum_age().clone(),
    );
    let mut store = WeatherStore::new(StoreConfig::default(), ProducerInstanceId::new(1))
        .expect("configuration must be valid");
    store
        .accept(
            WeatherIngress::product(
                WeatherProductId::new("radar"),
                SourceProductRevision::new(1),
                product,
            ),
            at(10),
        )
        .expect("product must be accepted");
    let snapshot = store.capture();
    let available = snapshot
        .snapshot()
        .product(&WeatherProductId::new("radar"))
        .and_then(|value| value.state().available())
        .expect("product must be available");
    assert_eq!(available.payload().time.ingress_monotonic.as_millis(), 10);
    let issue = available
        .issue_time()
        .as_present()
        .expect("issue time must be present");
    assert_eq!(issue.time.ingress_monotonic.as_millis(), 5);
    assert_eq!(issue.provenance, available.payload().provenance);
}
