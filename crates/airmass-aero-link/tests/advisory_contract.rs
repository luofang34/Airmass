//! Public advisory decoding and shape contract tests.

// The test exemptions in `clippy.toml` apply only below `cfg(test)`, and the
// helpers in this file are not `#[test]` functions.
#![cfg(test)]

use aero_link::{
    FisBContributor, FisBDeliveryPath, FisBFlags, FisBMessage, FisBOrigin, FisBProduct,
    FisBSourceTimeQuality, FisBTime,
};
use airmass_aero_link::AeroLinkAdapter;
use airmass_core::{
    AbsenceReason, AdvisoryAltitudeReference, AdvisoryGeometry, AdvisoryValidity, EvaluationTime,
    FieldState, MonotonicTime, ProducerInstanceId, StoreConfig, UtcTime, WeatherAdvisory,
    WeatherAdvisoryType, WeatherIngress, WeatherIngressEvent, WeatherProduct, WeatherStationId,
    WeatherStore,
};
use airmass_geojson::{
    FeatureDelta, FeatureRemovalReason, WeatherFeature, Wgs84Position, encode_feature_delta,
    map_snapshot_transition,
};
use serde::Deserialize;

const G_AIRMET: &str = include_str!("../test-data/advisories/recorded-g-airmet.json");
const TEXT_SIGMET: &str = include_str!("../test-data/advisories/recorded-text-sigmet.json");
const NO_BAND_G_AIRMET: &str =
    include_str!("../test-data/advisories/recorded-g-airmet-without-band.json");

#[derive(Debug, Deserialize)]
struct AdvisoryFixture {
    source_url: String,
    product_id: u16,
    issue_time: FixtureTime,
    twgo_payload_hex: String,
    raw_text: Option<String>,
    expected: ExpectedAdvisory,
}

#[derive(Clone, Copy, Debug, Deserialize)]
struct FixtureTime {
    month: u8,
    day: u8,
    hour: u8,
    minute: u8,
}

#[derive(Debug, Deserialize)]
struct ExpectedAdvisory {
    advisory_type: WeatherAdvisoryType,
    valid_start_unix_millis: i64,
    valid_end_unix_millis: i64,
    base_feet: Option<u32>,
    top_feet: Option<u32>,
    coordinates: Vec<[f64; 2]>,
}

#[test]
fn recorded_g_airmet_becomes_a_banded_shape_and_expires() {
    let fixture = fixture(G_AIRMET);
    assert!(fixture.source_url.contains("aviationweather.gov"));
    let ingress = adapt(&fixture);
    let product = product(&ingress);
    assert_eq!(
        product.issue_time().as_present().map(|field| field.value),
        Some(UtcTime::from_unix_millis(1_786_286_700_000))
    );
    let advisory = only_advisory(product);
    assert_advisory(advisory, &fixture.expected);
    let band = advisory
        .altitude_band()
        .as_present()
        .expect("the recorded G-AIRMET must carry its altitude band")
        .value;
    assert_eq!(band.base().feet(), 15_000);
    assert_eq!(band.top().feet(), 26_000);
    assert_eq!(
        band.base().reference(),
        AdvisoryAltitudeReference::MeanSeaLevel
    );

    let mut store = store();
    let current = store
        .accept(
            ingress,
            evaluation(fixture.expected.valid_start_unix_millis, 1_000_000),
        )
        .expect("the G-AIRMET must enter the store")
        .into_publication()
        .expect("the G-AIRMET must publish");
    let deltas = map_snapshot_transition(None, current.envelope(), &missing_station);
    let [FeatureDelta::Upsert(WeatherFeature::AdvisoryShape(shape))] = deltas.as_slice() else {
        panic!("the G-AIRMET must produce one shape upsert");
    };
    assert_eq!(shape.rings().len(), 1);
    assert_eq!(
        shape.rings()[0].positions().len(),
        fixture.expected.coordinates.len()
    );
    let encoded = encode_feature_delta(&deltas[0]).expect("the shape must encode as GeoJSON");
    let geojson: serde_json::Value =
        serde_json::from_str(&encoded).expect("the feature must be valid JSON");
    assert_eq!(geojson["geometry"]["type"], "Polygon");
    assert!(
        geojson["id"]
            .as_str()
            .is_some_and(|id| id.ends_with(":shape"))
    );
    assert_eq!(geojson["properties"]["feature_kind"], "advisory_shape");
    assert_eq!(
        geojson["properties"]["altitude_band"]["field"]["value"]["base"]["feet"],
        15_000
    );

    let expired = store
        .advance_time(evaluation(
            fixture.expected.valid_end_unix_millis,
            1_000_001,
        ))
        .expect("time must advance to the validity end")
        .into_publication()
        .expect("expiry must publish");
    let removals = map_snapshot_transition(
        Some(current.envelope()),
        expired.envelope(),
        &missing_station,
    );
    assert!(matches!(
        removals.as_slice(),
        [FeatureDelta::Remove {
            reason: FeatureRemovalReason::Expired,
            ..
        }]
    ));
}

#[test]
fn recorded_text_sigmet_uses_the_same_typed_shape_path() {
    let fixture = fixture(TEXT_SIGMET);
    assert!(
        fixture
            .raw_text
            .as_deref()
            .is_some_and(|text| text.contains("FAJO SIGMET C01"))
    );
    let ingress = adapt(&fixture);
    let product = product(&ingress);
    let advisory = only_advisory(product);
    assert_advisory(advisory, &fixture.expected);
    let band = advisory
        .altitude_band()
        .as_present()
        .expect("the text SIGMET must carry its altitude band")
        .value;
    assert_eq!(band.base().feet(), 24_000);
    assert_eq!(band.top().feet(), 39_000);
    assert_eq!(
        band.top().reference(),
        AdvisoryAltitudeReference::FlightLevel
    );

    let mut store = store();
    let current = store
        .accept(
            ingress,
            evaluation(fixture.expected.valid_start_unix_millis, 1_000_000),
        )
        .expect("the text SIGMET must enter the store")
        .into_publication()
        .expect("the text SIGMET must publish");
    let deltas = map_snapshot_transition(None, current.envelope(), &missing_station);
    let [FeatureDelta::Upsert(WeatherFeature::AdvisoryShape(shape))] = deltas.as_slice() else {
        panic!("the text SIGMET must produce one shape upsert");
    };
    assert_eq!(shape.rings()[0].positions().len(), 10);
    assert_eq!(
        shape
            .advisory()
            .advisory_type()
            .as_present()
            .map(|field| field.value),
        Some(WeatherAdvisoryType::Sigmet)
    );
}

#[test]
fn recorded_g_airmet_without_a_band_reports_no_limits() {
    let fixture = fixture(NO_BAND_G_AIRMET);
    let ingress = adapt(&fixture);
    let advisory = only_advisory(product(&ingress));
    assert_advisory(advisory, &fixture.expected);

    let FieldState::Absent(absence) = advisory.altitude_band() else {
        panic!("an advisory without source limits must not have an altitude band");
    };
    assert_eq!(absence.reason(), AbsenceReason::NotObserved);
    assert!(fixture.expected.base_feet.is_none());
    assert!(fixture.expected.top_feet.is_none());
}

#[test]
fn graphical_cwa_uses_the_product_specific_object_element_flag() {
    let mut fixture = fixture(G_AIRMET);
    fixture.product_id = 15;
    let mut payload = decode_hex(&fixture.twgo_payload_hex);
    let record_offset = 6;
    let label_bytes = if payload[record_offset + 4] & 1 == 0 {
        2
    } else {
        9
    };
    let object_offset = record_offset + 5 + label_bytes;
    let qualifier_bytes = if payload[object_offset] & 0x40 == 0 {
        0
    } else {
        3
    };
    let validity_offset = object_offset + 2 + qualifier_bytes + 2;
    payload[validity_offset..validity_offset + 8].copy_from_slice(&[8, 9, 16, 45, 8, 9, 18, 45]);
    let mut adapter = AeroLinkAdapter::default();
    let rejected = adapter.accept_message(&message(&fixture, payload.clone()), contributor(1));
    assert!(rejected.is_err());
    payload[object_offset] &= !0x80;

    let ingress = adapt_payload(&fixture, payload, 1_000_000);
    let advisory = only_advisory(product(&ingress));
    assert_eq!(
        advisory
            .advisory_type()
            .as_present()
            .map(|field| field.value),
        Some(WeatherAdvisoryType::CenterWeatherAdvisory)
    );
}

#[test]
fn a_newer_advisory_revision_replaces_the_shape() {
    let fixture = fixture(G_AIRMET);
    let first = adapt(&fixture);
    let mut changed_payload = decode_hex(&fixture.twgo_payload_hex);
    changed_payload[29] ^= 1;
    let replacement = adapt_payload(&fixture, changed_payload, 2_000_000);
    assert_eq!(first.product_id(), replacement.product_id());

    let mut store = store();
    let before = store
        .accept(
            first,
            evaluation(fixture.expected.valid_start_unix_millis, 1_000_000),
        )
        .expect("the first advisory must enter the store")
        .into_publication()
        .expect("the first advisory must publish");
    let after = store
        .accept(
            replacement,
            evaluation(fixture.expected.valid_start_unix_millis + 1, 2_000_000),
        )
        .expect("the newer advisory must enter the store")
        .into_publication()
        .expect("the newer advisory must publish");

    let deltas =
        map_snapshot_transition(Some(before.envelope()), after.envelope(), &missing_station);
    assert!(matches!(
        deltas.as_slice(),
        [FeatureDelta::Upsert(WeatherFeature::AdvisoryShape(_))]
    ));
}

fn assert_advisory(advisory: &WeatherAdvisory, expected: &ExpectedAdvisory) {
    assert_eq!(
        advisory
            .advisory_type()
            .as_present()
            .map(|field| field.value),
        Some(expected.advisory_type)
    );
    let validity = advisory
        .validity()
        .as_present()
        .expect("an advisory must have a validity interval")
        .value;
    let AdvisoryValidity::Period(validity) = validity else {
        panic!("the fixture must have interval validity");
    };
    assert_eq!(
        validity.start.as_unix_millis(),
        expected.valid_start_unix_millis
    );
    assert_eq!(
        validity.end.as_unix_millis(),
        expected.valid_end_unix_millis
    );
    let AdvisoryGeometry::Polygon(polygon) = &advisory
        .geometry()
        .as_present()
        .expect("an advisory must have geometry")
        .value
    else {
        panic!("the fixture must produce polygon geometry");
    };
    assert_eq!(polygon.rings().len(), 1);
    let positions = polygon.rings()[0].positions();
    assert_eq!(positions.len(), expected.coordinates.len());
    for (actual, expected) in positions.iter().zip(&expected.coordinates) {
        assert_close(f64::from(actual.latitude_e6()) / 1_000_000.0, expected[0]);
        assert_close(f64::from(actual.longitude_e6()) / 1_000_000.0, expected[1]);
    }
}

fn adapt(fixture: &AdvisoryFixture) -> WeatherIngress {
    adapt_payload(fixture, decode_hex(&fixture.twgo_payload_hex), 1_000_000)
}

fn adapt_payload(
    fixture: &AdvisoryFixture,
    payload: Vec<u8>,
    received_at_micros: u64,
) -> WeatherIngress {
    let mut adapter = AeroLinkAdapter::default();
    let batch = adapter
        .accept_message(&message(fixture, payload), contributor(received_at_micros))
        .expect("the recorded advisory must decode");
    assert_eq!(batch.values().len(), 1);
    batch.values()[0].clone()
}

fn message(fixture: &AdvisoryFixture, payload: Vec<u8>) -> FisBMessage {
    FisBMessage {
        flags: FisBFlags {
            a: false,
            g: false,
            p: false,
            segmented: false,
        },
        product: identified_product(fixture.product_id),
        time: FisBTime {
            month: Some(fixture.issue_time.month),
            day: Some(fixture.issue_time.day),
            hour: fixture.issue_time.hour,
            minute: fixture.issue_time.minute,
            second: None,
        },
        segmentation: None,
        payload,
        text_reports: Vec::new(),
    }
}

fn contributor(received_at_micros: u64) -> FisBContributor {
    FisBContributor {
        source_id: 7,
        source_epoch: 1,
        origin: FisBOrigin::Replay,
        delivery_path: FisBDeliveryPath::Replay,
        first_received_at_micros: received_at_micros,
        last_received_at_micros: received_at_micros,
        station_identity: None,
        source_time_quality: FisBSourceTimeQuality::LocalIngress,
        uncertainty_micros: Some(0),
    }
}

fn identified_product(id: u16) -> FisBProduct {
    let (name, format) = match id {
        12 => ("SIGMET and Convective SIGMET", "text_or_graphic"),
        14 => ("G-AIRMET", "text_or_graphic"),
        15 => ("Center weather advisory", "text_or_graphic"),
        _ => ("Unknown product", "unknown"),
    };
    let status = if format == "unknown" {
        "unassigned"
    } else {
        "standard"
    };
    serde_json::from_value(serde_json::json!({
        "id": id,
        "name": name,
        "format": format,
        "status": status,
    }))
    .expect("the fixture product metadata must match aero-link")
}

fn product(ingress: &WeatherIngress) -> &WeatherProduct {
    let WeatherIngressEvent::Product(product) =
        ingress.event().expect("the adapter schema must be current")
    else {
        panic!("the active fixture must produce a weather product");
    };
    product
}

fn only_advisory(product: &WeatherProduct) -> &WeatherAdvisory {
    let advisories = product
        .typed_value()
        .as_present()
        .and_then(|field| field.value.advisories())
        .expect("the product must use the typed advisory value");
    let [advisory] = advisories else {
        panic!("the fixture must contain one advisory");
    };
    advisory
}

fn fixture(value: &str) -> AdvisoryFixture {
    serde_json::from_str(value).expect("the advisory fixture must be valid")
}

fn decode_hex(value: &str) -> Vec<u8> {
    value
        .as_bytes()
        .chunks_exact(2)
        .map(|pair| {
            let text = core::str::from_utf8(pair).expect("the fixture must contain ASCII hex");
            u8::from_str_radix(text, 16).expect("the fixture must contain valid hex")
        })
        .collect()
}

fn store() -> WeatherStore {
    WeatherStore::new(StoreConfig::default(), ProducerInstanceId::new(41))
        .expect("the default store must be valid")
}

fn evaluation(utc_millis: i64, monotonic_micros: u64) -> EvaluationTime {
    EvaluationTime::new(
        UtcTime::from_unix_millis(utc_millis),
        MonotonicTime::from_micros(monotonic_micros),
    )
}

fn missing_station(_: &WeatherStationId) -> Option<Wgs84Position> {
    None
}

fn assert_close(actual: f64, expected: f64) {
    assert!((actual - expected).abs() < 0.001, "{actual} != {expected}");
}
