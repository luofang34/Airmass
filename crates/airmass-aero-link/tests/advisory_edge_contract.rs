//! Advisory protocol edge-case contract tests.

#![allow(clippy::expect_used, clippy::panic)]

use aero_link::{
    FisBContributor, FisBDeliveryPath, FisBFlags, FisBMessage, FisBOrigin, FisBProduct,
    FisBSourceTimeQuality, FisBTime,
};
use airmass_aero_link::AeroLinkAdapter;
use airmass_core::{
    AdvisoryGeometry, AdvisoryValidity, EvaluationTime, MonotonicTime, ProducerInstanceId,
    StoreConfig, UtcTime, WeatherAdvisory, WeatherAdvisoryType, WeatherIngress,
    WeatherIngressEvent, WeatherProduct, WeatherStationId, WeatherStore,
};
use airmass_geojson::{
    FeatureDelta, WeatherFeature, Wgs84Position, encode_feature_delta, map_snapshot_transition,
};

const G_AIRMET: &str = include_str!("../test-data/advisories/recorded-g-airmet.json");
const NO_BAND_G_AIRMET: &str =
    include_str!("../test-data/advisories/recorded-g-airmet-without-band.json");
const INDEPENDENT_GOLDEN: &str =
    include_str!("../test-data/advisories/independent-golden-g-airmet.json");

#[test]
fn independent_golden_g_airmet_matches_its_decoder_output() {
    let fixture: serde_json::Value =
        serde_json::from_str(INDEPENDENT_GOLDEN).expect("golden fixture must be JSON");
    assert_eq!(
        fixture["source_kind"],
        "independent standards-based golden vector"
    );
    assert!(
        fixture["source_url"]
            .as_str()
            .is_some_and(|url| url.contains("c3d3b0473429336b1a98672b2e8b276c24d49765"))
    );
    let payload = decode_hex(
        fixture["twgo_payload_hex"]
            .as_str()
            .expect("golden fixture must contain hex"),
    );
    let mut source = message(14, payload);
    source.time.month = Some(10);
    source.time.day = Some(30);
    source.time.hour = 2;
    source.time.minute = 45;
    let mut adapter = AeroLinkAdapter::default();
    let batch = adapter
        .accept_message(&source, contributor(1_000_000))
        .expect("the independent golden vector must decode");
    let advisory = only_advisory(product(&batch.values()[0]));
    assert_eq!(
        advisory
            .altitude()
            .as_present()
            .map(|field| field.value.feet()),
        Some(16_000)
    );
    assert!(matches!(
        advisory.validity().as_present().map(|field| field.value),
        Some(AdvisoryValidity::Point(value))
            if value == UtcTime::from_unix_millis(1_604_048_400_000)
    ));
    let expected = fixture["expected"]["coordinates"]
        .as_array()
        .expect("golden fixture must contain coordinates");
    let actual = polygon_ring(advisory);
    assert_eq!(actual.len(), expected.len());
    for (position, expected) in actual.iter().zip(expected) {
        let latitude = expected[0].as_f64().expect("latitude must be numeric");
        let longitude = expected[1].as_f64().expect("longitude must be numeric");
        assert!((f64::from(position.latitude_e6()) / 1_000_000.0 - latitude).abs() < 0.001);
        assert!((f64::from(position.longitude_e6()) / 1_000_000.0 - longitude).abs() < 0.001);
    }
}

#[test]
fn every_legacy_advisory_product_maps_to_a_typed_value() {
    for (product_id, expected_type) in [
        (2, WeatherAdvisoryType::Sigmet),
        (22, WeatherAdvisoryType::Sigmet),
        (3, WeatherAdvisoryType::ConvectiveSigmet),
        (23, WeatherAdvisoryType::ConvectiveSigmet),
        (4, WeatherAdvisoryType::Airmet),
        (24, WeatherAdvisoryType::Airmet),
        (6, WeatherAdvisoryType::AviationWeatherWarning),
        (26, WeatherAdvisoryType::AviationWeatherWarning),
    ] {
        let text = legacy_text(product_id);
        let payload = if product_id >= 20 {
            encode_dlac(text)
        } else {
            text.as_bytes().to_vec()
        };
        let mut input = message(product_id, payload);
        input.time.month = None;
        input.time.day = None;
        input.time.second = (product_id % 2 != 0).then_some(30);
        let mut adapter = AeroLinkAdapter::default();
        let batch = adapter
            .accept_message_at(
                &input,
                contributor(1_000_000 + u64::from(product_id)),
                UtcTime::from_unix_millis(1_786_330_800_000),
            )
            .expect("a legacy advisory must decode with caller UTC");
        let product = product(&batch.values()[0]);
        let advisory = only_advisory(product);
        assert_eq!(
            advisory
                .advisory_type()
                .as_present()
                .map(|field| field.value),
            Some(expected_type),
            "product {product_id}"
        );
    }
}

#[test]
fn legacy_advisory_requires_caller_utc() {
    let mut adapter = AeroLinkAdapter::default();
    let result = adapter.accept_message(
        &message(2, legacy_text(2).as_bytes().to_vec()),
        contributor(1_000_000),
    );
    assert!(result.is_err());
}

#[test]
fn cancellation_at_another_wire_index_uses_the_same_lifecycle_identity() {
    let text = legacy_text(12);
    let active_payload = twgo_text_file(&[(77, true, text)]);
    let cancellation_payload = twgo_text_file(&[(99, false, ""), (77, false, "")]);
    let mut adapter = AeroLinkAdapter::default();
    let active = adapter
        .accept_message(&message(12, active_payload), contributor(1_000_000))
        .expect("the active report must decode")
        .values()[0]
        .clone();
    let cancellations = adapter
        .accept_message(&message(12, cancellation_payload), contributor(2_000_000))
        .expect("the cancellation file must decode");
    let target = cancellations
        .values()
        .iter()
        .find(|ingress| ingress.product_id() == active.product_id())
        .expect("the second wire record must cancel the active report");
    assert!(matches!(
        target.event().expect("the schema must be current"),
        WeatherIngressEvent::Cancellation(_)
    ));

    let mut store = store();
    store
        .accept(active, evaluation(1_786_330_500_000, 1_000_000))
        .expect("the active report must enter the store");
    let publication = store
        .accept(target.clone(), evaluation(1_786_331_280_001, 2_000_000))
        .expect("the cancellation must enter the same lifecycle")
        .into_publication()
        .expect("the cancellation must publish");
    assert!(
        publication.envelope().snapshot().products()[0]
            .state()
            .available()
            .is_none()
    );
}

#[test]
fn graphical_fragments_form_one_shape_before_ring_closure() {
    let payload = fixture_payload(G_AIRMET);
    let fragmented = split_graphical_record(&payload, 10, 1);
    let original = adapt(14, payload, 1_000_000);
    let combined = adapt(14, fragmented, 2_000_000);
    let original_ring = polygon_ring(only_advisory(product(&original)));
    let combined_ring = polygon_ring(only_advisory(product(&combined)));
    assert_eq!(combined_ring, original_ring);
    assert_eq!(combined.product_id(), original.product_id());
}

#[test]
fn graphical_fragment_sequence_must_be_contiguous() {
    let payload = fixture_payload(G_AIRMET);
    let fragmented = split_graphical_record(&payload, 10, 2);
    let mut adapter = AeroLinkAdapter::default();
    let result = adapter.accept_message(&message(14, fragmented), contributor(1_000_000));
    assert!(result.is_err());
}

#[test]
fn equal_g_airmet_applicability_is_a_point_forecast() {
    let mut payload = fixture_payload(G_AIRMET);
    let validity = 6 + 11;
    payload.copy_within(validity..validity + 4, validity + 4);
    let ingress = adapt(14, payload, 1_000_000);
    let product = product(&ingress);
    let advisory = only_advisory(product);
    let point = advisory.validity().as_present().map(|field| field.value);
    let Some(AdvisoryValidity::Point(point)) = point else {
        panic!("equal applicability must produce point validity");
    };
    let lifecycle = product
        .valid_period()
        .as_present()
        .expect("a point forecast must have a lifecycle interval")
        .value;
    assert_eq!(lifecycle.start, point);
    assert_eq!(lifecycle.end.as_unix_millis(), point.as_unix_millis() + 1);

    let mut store = store();
    store
        .accept(ingress, evaluation(point.as_unix_millis(), 1_000_000))
        .expect("the point forecast must enter the store");
    let expired = store
        .advance_time(evaluation(point.as_unix_millis() + 1, 1_000_001))
        .expect("the point forecast must expire")
        .into_publication();
    assert!(expired.is_some());
}

#[test]
fn polyline_keeps_its_altitude_and_encodes_a_line_string() {
    let mut payload = fixture_payload(NO_BAND_G_AIRMET);
    let timing = 6 + 9;
    payload[timing] = (payload[timing] & 0xf0) | 11;
    let ingress = adapt(14, payload, 1_000_000);
    let advisory = only_advisory(product(&ingress));
    assert!(matches!(
        advisory.geometry().as_present().map(|field| &field.value),
        Some(AdvisoryGeometry::Polyline(_))
    ));
    assert_eq!(
        advisory
            .altitude()
            .as_present()
            .map(|field| field.value.feet()),
        Some(0)
    );
    assert!(advisory.altitude_band().as_present().is_none());

    let start = product(&ingress)
        .valid_period()
        .as_present()
        .expect("the polyline must have lifecycle validity")
        .value
        .start;
    let mut store = store();
    let current = store
        .accept(ingress, evaluation(start.as_unix_millis(), 1_000_000))
        .expect("the polyline must enter the store")
        .into_publication()
        .expect("the polyline must publish");
    let deltas = map_snapshot_transition(None, current.envelope(), &missing_station);
    let [FeatureDelta::Upsert(WeatherFeature::AdvisoryShape(shape))] = deltas.as_slice() else {
        panic!("the polyline must produce one shape");
    };
    assert!(shape.line().is_some());
    let encoded = encode_feature_delta(&deltas[0]).expect("the line must encode");
    let value: serde_json::Value =
        serde_json::from_str(&encoded).expect("the feature must be JSON");
    assert_eq!(value["geometry"]["type"], "LineString");
    assert_eq!(value["properties"]["altitude"]["field"]["value"]["feet"], 0);
}

fn split_graphical_record(payload: &[u8], split: usize, second_id: u8) -> Vec<u8> {
    let record = &payload[6..];
    let vertex_offset = 19;
    let vertices = &record[vertex_offset..];
    let first_end = split * 6;
    let first = graphical_fragment(record, &vertices[..first_end], 0, split);
    let second_count = vertices.len() / 6 - split;
    let second = graphical_fragment(record, &vertices[first_end..], second_id, second_count);
    let mut output = payload[..6].to_vec();
    output[1] = (2 << 4) | (output[1] & 0x0f);
    output.extend_from_slice(&first);
    output.extend_from_slice(&second);
    output
}

fn graphical_fragment(record: &[u8], vertices: &[u8], id: u8, count: usize) -> Vec<u8> {
    let mut fragment = record[..19].to_vec();
    fragment[4] = (fragment[4] & 1) | (id << 1);
    fragment[10] = (fragment[10] & 0xc0) | (u8::try_from(count).expect("count must fit") - 1);
    fragment.extend_from_slice(vertices);
    let length = fragment.len();
    fragment[0] = u8::try_from(length >> 2).expect("record length must fit");
    fragment[1] =
        (fragment[1] & 0x3f) | (u8::try_from(length & 3).expect("record remainder must fit") << 6);
    fragment
}

fn twgo_text_file(records: &[(u16, bool, &str)]) -> Vec<u8> {
    let mut payload = vec![
        0x21,
        u8::try_from(records.len()).expect("record count must fit") << 4,
        0,
        0,
        0,
        0xff,
    ];
    for (report_number, active, text) in records {
        let encoded = encode_dlac(text);
        let length = encoded.len() + 5;
        payload.extend_from_slice(
            &u16::try_from(length)
                .expect("length must fit")
                .to_be_bytes(),
        );
        payload.push(u8::try_from(report_number >> 6).expect("report number must fit"));
        payload.push(u8::try_from((report_number & 0x3f) << 2).expect("report number must fit"));
        payload.push((26 << 3) | if *active { 4 } else { 0 });
        payload.extend_from_slice(&encoded);
    }
    payload
}

fn encode_dlac(text: &str) -> Vec<u8> {
    let mut bytes = Vec::new();
    let mut accumulator = 0_u32;
    let mut pending_bits = 0_u8;
    for symbol in text.chars().map(|character| match character {
        'A'..='Z' => (character as u8) - b'A' + 1,
        ' '..='?' => character as u8,
        _ => 0,
    }) {
        accumulator = (accumulator << 6) | u32::from(symbol);
        pending_bits = pending_bits.wrapping_add(6);
        while pending_bits >= 8 {
            pending_bits -= 8;
            bytes.push(((accumulator >> pending_bits) & 0xff) as u8);
        }
    }
    if pending_bits > 0 {
        bytes.push(((accumulator << (8 - pending_bits)) & 0xff) as u8);
    }
    bytes
}

fn legacy_text(product_id: u16) -> &'static str {
    match product_id {
        2 | 22 | 12 => concat!(
            "FAJO SIGMET C01 VALID 100255/100600 FAOR WI ",
            "S3508 E03225 - S4019 E04007 - S4828 E05255 FL240/390="
        ),
        3 | 23 => concat!(
            "CONVECTIVE SIGMET 12C VALID 100255/100600 WI ",
            "N3508 W03225 - N4019 W04007 - N4828 W05255 FL240/390="
        ),
        4 | 24 => concat!(
            "WAUS AIRMET SIERRA VALID 100255/100600 WI ",
            "N3508 W03225 - N4019 W04007 - N4828 W05255 FL120/180="
        ),
        6 | 26 => concat!(
            "WWUS WARNING W01 VALID 100255/100600 WI ",
            "N3508 W03225 - N4019 W04007 - N4828 W05255 FL120/180="
        ),
        _ => panic!("the test product must have text"),
    }
}

fn fixture_payload(fixture: &str) -> Vec<u8> {
    let value: serde_json::Value = serde_json::from_str(fixture).expect("fixture must be JSON");
    decode_hex(
        value["twgo_payload_hex"]
            .as_str()
            .expect("fixture must contain hex"),
    )
}

fn decode_hex(value: &str) -> Vec<u8> {
    value
        .as_bytes()
        .chunks_exact(2)
        .map(|pair| {
            let text = core::str::from_utf8(pair).expect("hex must be ASCII");
            u8::from_str_radix(text, 16).expect("fixture hex must be valid")
        })
        .collect()
}

fn adapt(product_id: u16, payload: Vec<u8>, received: u64) -> WeatherIngress {
    let mut adapter = AeroLinkAdapter::default();
    let batch = adapter
        .accept_message(&message(product_id, payload), contributor(received))
        .expect("the advisory must decode");
    assert_eq!(batch.values().len(), 1);
    batch.values()[0].clone()
}

fn message(product_id: u16, payload: Vec<u8>) -> FisBMessage {
    FisBMessage {
        flags: FisBFlags {
            a: false,
            g: false,
            p: false,
            segmented: false,
        },
        product: identified_product(product_id),
        time: FisBTime {
            month: Some(8),
            day: Some(10),
            hour: 3,
            minute: 8,
            second: None,
        },
        segmentation: None,
        payload,
        text_reports: Vec::new(),
    }
}

fn identified_product(id: u16) -> FisBProduct {
    let (name, format) = match id {
        2 | 22 => ("SIGMET", if id < 20 { "text" } else { "dlac_text" }),
        3 | 23 => (
            "Convective SIGMET",
            if id < 20 { "text" } else { "dlac_text" },
        ),
        4 | 24 => ("AIRMET", if id < 20 { "text" } else { "dlac_text" }),
        6 | 26 => (
            "Aviation weather warning",
            if id < 20 { "text" } else { "dlac_text" },
        ),
        12 => ("SIGMET and Convective SIGMET", "text_or_graphic"),
        14 => ("G-AIRMET", "text_or_graphic"),
        _ => ("Unknown product", "unknown"),
    };
    serde_json::from_value(serde_json::json!({
        "id": id,
        "name": name,
        "format": format,
        "status": if format == "unknown" { "unassigned" } else { "standard" },
    }))
    .expect("product metadata must match aero-link")
}

fn contributor(received: u64) -> FisBContributor {
    FisBContributor {
        source_id: 7,
        source_epoch: 1,
        origin: FisBOrigin::Replay,
        delivery_path: FisBDeliveryPath::Replay,
        first_received_at_micros: received,
        last_received_at_micros: received,
        station_identity: None,
        source_time_quality: FisBSourceTimeQuality::LocalIngress,
        uncertainty_micros: Some(0),
    }
}

fn product(ingress: &WeatherIngress) -> &WeatherProduct {
    let WeatherIngressEvent::Product(product) = ingress.event().expect("schema must be current")
    else {
        panic!("the event must be a product");
    };
    product
}

fn only_advisory(product: &WeatherProduct) -> &WeatherAdvisory {
    let values = product
        .typed_value()
        .as_present()
        .and_then(|field| field.value.advisories())
        .expect("the value must contain advisories");
    let [advisory] = values else {
        panic!("the product must contain one advisory");
    };
    advisory
}

fn polygon_ring(advisory: &WeatherAdvisory) -> &[airmass_core::AdvisoryPosition] {
    let Some(field) = advisory.geometry().as_present() else {
        panic!("the advisory must have geometry");
    };
    let AdvisoryGeometry::Polygon(polygon) = &field.value else {
        panic!("the advisory must be a polygon");
    };
    polygon.rings()[0].positions()
}

fn store() -> WeatherStore {
    WeatherStore::new(StoreConfig::default(), ProducerInstanceId::new(51))
        .expect("the store configuration must be valid")
}

fn evaluation(utc: i64, monotonic: u64) -> EvaluationTime {
    EvaluationTime::new(
        UtcTime::from_unix_millis(utc),
        MonotonicTime::from_micros(monotonic),
    )
}

fn missing_station(_: &WeatherStationId) -> Option<Wgs84Position> {
    None
}
