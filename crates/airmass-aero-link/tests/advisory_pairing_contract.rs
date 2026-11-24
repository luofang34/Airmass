//! Product-specific TWGO pairing contract tests.

// The test exemptions in `clippy.toml` apply only below `cfg(test)`, and the
// helpers in this file are not `#[test]` functions.
#![cfg(test)]

use aero_link::{
    FisBContributor, FisBDeliveryPath, FisBFlags, FisBMessage, FisBOrigin, FisBProduct,
    FisBSourceTimeQuality, FisBTime, Product8RecordData, decode_product_8,
};
use airmass_aero_link::{AdapterError, AdvisoryDecodeError, AeroLinkAdapter, MappingError};
use airmass_core::{
    AdvisoryGeometry, AdvisoryValidity, FieldState, WeatherAdvisory, WeatherAdvisoryType,
    WeatherIngress, WeatherIngressEvent, WeatherProduct,
};
use serde::Deserialize;

const CONVECTIVE_PAIR: &str =
    include_str!("../test-data/advisories/recorded-convective-sigmet-pair.json");
const AIRMET_GRAPHIC: &str = include_str!("../test-data/advisories/recorded-airmet-graphical.json");
const G_AIRMET: &str = include_str!("../test-data/advisories/recorded-g-airmet.json");

#[derive(Clone, Copy, Debug, Deserialize)]
struct FixtureTime {
    month: u8,
    day: u8,
    hour: u8,
    minute: u8,
}

#[derive(Debug, Deserialize)]
struct PairFixture {
    source_url: String,
    source_commit: String,
    product_id: u16,
    report_year: u8,
    report_number: u16,
    graphical_received_unix_millis: u64,
    text_received_unix_millis: u64,
    issue_time: FixtureTime,
    graphical_payload_hex: String,
    text_payload_hex: String,
}

#[derive(Debug, Deserialize)]
struct GraphicalFixture {
    source_url: String,
    source_commit: String,
    product_id: u16,
    issue_time: FixtureTime,
    text_issue_time: FixtureTime,
    graphical_payload_hex: String,
    text_payload_hex: String,
}

#[test]
fn recorded_product_12_graphic_waits_for_its_convective_text() {
    let fixture = pair_fixture();
    assert_recorded_pair(&fixture);
    let graphical_time = fixture.graphical_received_unix_millis * 1_000;
    let text_time = fixture.text_received_unix_millis * 1_000;
    let mut adapter = AeroLinkAdapter::default();

    let pending = adapter
        .accept_message(
            &message(
                fixture.product_id,
                fixture.issue_time,
                decode_hex(&fixture.graphical_payload_hex),
            ),
            contributor(7, 1, graphical_time),
        )
        .expect("the recorded graphical APDU must enter the pairing cache");
    assert!(pending.values().is_empty());

    let matched = adapter
        .accept_message(
            &message(
                fixture.product_id,
                fixture.issue_time,
                decode_hex(&fixture.text_payload_hex),
            ),
            contributor(7, 1, text_time),
        )
        .expect("the matching text APDU must release the graphical advisory");
    assert_eq!(matched.values().len(), 1);
    assert_paired_evidence(&matched.values()[0], graphical_time, text_time);
}

#[test]
fn recorded_product_12_pair_maps_in_text_first_order() {
    let fixture = pair_fixture();
    let mut adapter = AeroLinkAdapter::default();
    adapter
        .accept_message(
            &message(
                fixture.product_id,
                fixture.issue_time,
                decode_hex(&fixture.text_payload_hex),
            ),
            contributor(7, 1, 1_000_000),
        )
        .expect("the text APDU must cache its advisory type");
    let matched = adapter
        .accept_message(
            &message(
                fixture.product_id,
                fixture.issue_time,
                decode_hex(&fixture.graphical_payload_hex),
            ),
            contributor(7, 1, 2_000_000),
        )
        .expect("the later graphical APDU must use the cached type");
    assert_eq!(matched.values().len(), 1);
    assert_eq!(
        advisory(&matched.values()[0])
            .advisory_type()
            .as_present()
            .map(|field| field.value),
        Some(WeatherAdvisoryType::ConvectiveSigmet)
    );
}

#[test]
fn recorded_product_11_graphic_uses_its_zero_element_flag() {
    let fixture: GraphicalFixture =
        serde_json::from_str(AIRMET_GRAPHIC).expect("the AIRMET fixture must be valid");
    assert!(fixture.source_url.contains("rand-projects/fisb-decode"));
    assert_eq!(fixture.source_commit.len(), 40);
    let mut adapter = AeroLinkAdapter::default();
    let text_payload = decode_hex(&fixture.text_payload_hex);
    let decoded_text = decode_product_8(&text_payload).expect("the Product 11 text must decode");
    let Product8RecordData::Text { text, .. } = &decoded_text.records[0].data else {
        panic!("the Product 11 text fixture must contain text");
    };
    assert!(text.contains("AIRMET"));
    let text_batch = adapter
        .accept_message(
            &message(fixture.product_id, fixture.text_issue_time, text_payload),
            contributor(7, 1, 900_000),
        )
        .expect("the recorded Product 11 text must not fail mapping");
    assert!(text_batch.values().is_empty());
    let batch = adapter
        .accept_message(
            &message(
                fixture.product_id,
                fixture.issue_time,
                decode_hex(&fixture.graphical_payload_hex),
            ),
            contributor(7, 1, 1_000_000),
        )
        .expect("the recorded Product 11 graphic must map");
    assert_eq!(batch.values().len(), 1);
    assert_eq!(
        advisory(&batch.values()[0])
            .advisory_type()
            .as_present()
            .map(|field| field.value),
        Some(WeatherAdvisoryType::Airmet)
    );
}

#[test]
fn product_15_rejects_polyline_geometry() {
    let (mut payload, issue) = g_airmet_payload();
    let (object, timing, _) = graphical_offsets(&payload);
    payload[object] &= !0x80;
    payload[timing] = (payload[timing] & 0xf0) | 11;
    let mut adapter = AeroLinkAdapter::default();
    let error = adapter
        .accept_message(&message(15, issue, payload), contributor(7, 1, 1_000_000))
        .expect_err("Product 15 must reject polyline geometry");
    assert!(matches!(
        error,
        AdapterError::Mapping(MappingError::Advisory {
            source: AdvisoryDecodeError::UnsupportedGraphicalField {
                field: "overlay geometry option",
                value: 11,
            },
            ..
        })
    ));
}

#[test]
fn product_15_rejects_zero_duration_validity() {
    let (mut payload, issue) = g_airmet_payload();
    let (object, _, validity) = graphical_offsets(&payload);
    payload[object] &= !0x80;
    payload.copy_within(validity..validity + 4, validity + 4);
    let mut adapter = AeroLinkAdapter::default();
    let error = adapter
        .accept_message(&message(15, issue, payload), contributor(7, 1, 1_000_000))
        .expect_err("Product 15 must reject zero-duration validity");
    assert!(matches!(
        error,
        AdapterError::Mapping(MappingError::Advisory {
            source: AdvisoryDecodeError::InvalidValidity { .. },
            ..
        })
    ));
}

#[test]
fn graphical_polyline_rejects_mixed_vertex_altitudes() {
    let (mut payload, issue) = g_airmet_payload();
    let (_, timing, _) = graphical_offsets(&payload);
    payload[timing] = (payload[timing] & 0xf0) | 11;
    let mut adapter = AeroLinkAdapter::default();
    let error = adapter
        .accept_message(&message(14, issue, payload), contributor(7, 1, 1_000_000))
        .expect_err("a polyline must have one altitude");
    assert!(matches!(
        error,
        AdapterError::Mapping(MappingError::Advisory {
            source: AdvisoryDecodeError::InconsistentGraphicalField {
                field: "polyline altitude",
            },
            ..
        })
    ));
}

#[test]
fn graphical_applicability_uses_the_year_nearest_its_issue_time() {
    let (mut january_payload, _) = g_airmet_payload();
    set_applicability(&mut january_payload, [1, 1, 0, 0], [1, 1, 1, 0]);
    let january = adapt_graphic(
        january_payload,
        FixtureTime {
            month: 12,
            day: 31,
            hour: 23,
            minute: 55,
        },
    );
    assert_period(&january, 1_798_761_600_000, 1_798_765_200_000);

    let (mut december_payload, _) = g_airmet_payload();
    set_applicability(&mut december_payload, [12, 31, 23, 0], [1, 1, 1, 0]);
    let december = adapt_graphic(
        december_payload,
        FixtureTime {
            month: 1,
            day: 1,
            hour: 0,
            minute: 5,
        },
    );
    assert_period(&december, 1_767_222_000_000, 1_767_229_200_000);
}

fn assert_recorded_pair(fixture: &PairFixture) {
    assert!(fixture.source_url.contains("rand-projects/fisb-decode"));
    assert_eq!(fixture.source_commit.len(), 40);
    let graphical = decode_product_8(&decode_hex(&fixture.graphical_payload_hex))
        .expect("the recorded graphical file must decode");
    let text = decode_product_8(&decode_hex(&fixture.text_payload_hex))
        .expect("the recorded text file must decode");
    let Product8RecordData::Graphical {
        report_year,
        report_number,
    } = graphical.records[0].data
    else {
        panic!("the graphical fixture must contain a graphical record");
    };
    assert_eq!(
        (report_year, report_number),
        (fixture.report_year, fixture.report_number)
    );
    let Product8RecordData::Text { text, .. } = &text.records[0].data else {
        panic!("the text fixture must contain a text record");
    };
    assert!(text.contains("CONVECTIVE SIGMET 27E"));
}

fn assert_paired_evidence(ingress: &WeatherIngress, graphical_time: u64, text_time: u64) {
    let advisory = advisory(ingress);
    let advisory_type = advisory
        .advisory_type()
        .as_present()
        .expect("the paired advisory must have a type");
    let geometry = advisory
        .geometry()
        .as_present()
        .expect("the paired advisory must have geometry");
    assert_eq!(advisory_type.value, WeatherAdvisoryType::ConvectiveSigmet);
    assert!(matches!(geometry.value, AdvisoryGeometry::Polygon(_)));
    assert_eq!(advisory_type.time.ingress_monotonic.as_micros(), text_time);
    assert_eq!(geometry.time.ingress_monotonic.as_micros(), graphical_time);
    assert_ne!(
        advisory_type.provenance.raw_product,
        geometry.provenance.raw_product
    );
    assert_eq!(ingress.source_product_revision().get(), text_time << 10);
}

fn advisory(ingress: &WeatherIngress) -> &WeatherAdvisory {
    let WeatherIngressEvent::Product(product) =
        ingress.event().expect("the paired ingress must be current")
    else {
        panic!("the paired ingress must contain a product");
    };
    only_advisory(product)
}

fn only_advisory(product: &WeatherProduct) -> &WeatherAdvisory {
    let FieldState::Present(values) = product.typed_value() else {
        panic!("the paired product must have a typed value");
    };
    values
        .value
        .advisories()
        .and_then(|values| values.first())
        .expect("the paired product must contain one advisory")
}

fn pair_fixture() -> PairFixture {
    serde_json::from_str(CONVECTIVE_PAIR).expect("the Product 12 fixture must be valid")
}

fn g_airmet_payload() -> (Vec<u8>, FixtureTime) {
    let value: serde_json::Value =
        serde_json::from_str(G_AIRMET).expect("the G-AIRMET fixture must be valid");
    let issue = serde_json::from_value(value["issue_time"].clone())
        .expect("the G-AIRMET fixture must contain its issue time");
    let payload = decode_hex(
        value["twgo_payload_hex"]
            .as_str()
            .expect("the G-AIRMET fixture must contain hex"),
    );
    (payload, issue)
}

fn graphical_offsets(payload: &[u8]) -> (usize, usize, usize) {
    let record = 6;
    let label_bytes = if payload[record + 4] & 1 == 0 { 2 } else { 9 };
    let object = record + 5 + label_bytes;
    let qualifier_bytes = if payload[object] & 0x40 == 0 { 0 } else { 3 };
    let timing = object + 2 + qualifier_bytes;
    (object, timing, timing + 2)
}

fn set_applicability(payload: &mut [u8], start: [u8; 4], end: [u8; 4]) {
    let (_, _, validity) = graphical_offsets(payload);
    payload[validity..validity + 4].copy_from_slice(&start);
    payload[validity + 4..validity + 8].copy_from_slice(&end);
}

fn adapt_graphic(payload: Vec<u8>, issue: FixtureTime) -> WeatherIngress {
    let mut adapter = AeroLinkAdapter::default();
    let batch = adapter
        .accept_message(&message(14, issue, payload), contributor(7, 1, 1_000_000))
        .expect("the New Year graphical advisory must map");
    assert_eq!(batch.values().len(), 1);
    batch.values()[0].clone()
}

fn assert_period(ingress: &WeatherIngress, start: i64, end: i64) {
    let value = advisory(ingress)
        .validity()
        .as_present()
        .expect("the advisory must have validity")
        .value;
    let AdvisoryValidity::Period(period) = value else {
        panic!("the advisory must have period validity");
    };
    assert_eq!(period.start.as_unix_millis(), start);
    assert_eq!(period.end.as_unix_millis(), end);
}

fn message(id: u16, issue: FixtureTime, payload: Vec<u8>) -> FisBMessage {
    FisBMessage {
        flags: FisBFlags {
            a: false,
            g: false,
            p: false,
            segmented: false,
        },
        product: identified_product(id),
        time: FisBTime {
            month: Some(issue.month),
            day: Some(issue.day),
            hour: issue.hour,
            minute: issue.minute,
            second: None,
        },
        segmentation: None,
        payload,
        text_reports: Vec::new(),
    }
}

fn contributor(source_id: u32, source_epoch: u32, time: u64) -> FisBContributor {
    FisBContributor {
        source_id,
        source_epoch,
        origin: FisBOrigin::Replay,
        delivery_path: FisBDeliveryPath::Replay,
        first_received_at_micros: time,
        last_received_at_micros: time,
        station_identity: None,
        source_time_quality: FisBSourceTimeQuality::LocalIngress,
        uncertainty_micros: Some(0),
    }
}

fn identified_product(id: u16) -> FisBProduct {
    let name = match id {
        11 => "AIRMET",
        12 => "SIGMET and Convective SIGMET",
        14 => "G-AIRMET",
        15 => "Center weather advisory",
        _ => "Unknown product",
    };
    serde_json::from_value(serde_json::json!({
        "id": id,
        "name": name,
        "format": "text_or_graphic",
        "status": "standard",
    }))
    .expect("the fixture product metadata must match aero-link")
}

fn decode_hex(value: &str) -> Vec<u8> {
    assert_eq!(value.len() % 2, 0);
    value
        .as_bytes()
        .chunks_exact(2)
        .map(|pair| {
            let text = core::str::from_utf8(pair).expect("the fixture must contain ASCII hex");
            u8::from_str_radix(text, 16).expect("the fixture must contain valid hex")
        })
        .collect()
}
