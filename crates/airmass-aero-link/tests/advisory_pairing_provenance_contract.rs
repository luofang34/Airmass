//! Multi-contributor Product 12 pairing contract tests.

// The test exemptions in `clippy.toml` apply only below `cfg(test)`, and the
// helpers in this file are not `#[test]` functions.
#![cfg(test)]

use aero_link::{
    FisBContributor, FisBDeliveryPath, FisBFlags, FisBMessage, FisBOrigin, FisBProduct,
    FisBSegmentation, FisBSourceTimeQuality, FisBTime,
};
use airmass_aero_link::AeroLinkAdapter;
use airmass_core::{
    AdvisoryValidity, EvaluationTime, MonotonicTime, ProducerInstanceId, StoreConfig, UtcTime,
    WeatherAdvisory, WeatherIngress, WeatherIngressEvent, WeatherProduct, WeatherStore,
};
use serde::Deserialize;

const CONVECTIVE_PAIR: &str =
    include_str!("../test-data/advisories/recorded-convective-sigmet-pair.json");

#[derive(Clone, Copy, Debug, Deserialize)]
struct FixtureTime {
    month: u8,
    day: u8,
    hour: u8,
    minute: u8,
}

#[derive(Debug, Deserialize)]
struct PairFixture {
    product_id: u16,
    issue_time: FixtureTime,
    graphical_payload_hex: String,
    text_payload_hex: String,
}

#[test]
fn a_pair_emits_only_contributors_present_in_both_records() {
    let fixture: PairFixture =
        serde_json::from_str(CONVECTIVE_PAIR).expect("the Product 12 fixture must be valid");
    let graphical = decode_hex(&fixture.graphical_payload_hex);
    let split = graphical.len() / 2;
    let mut adapter = AeroLinkAdapter::default();

    let first_segment = adapter
        .accept_message(
            &segmented_message(&fixture, 2, &graphical[split..]),
            contributor(7, 1_000_000),
        )
        .expect("the first graphical segment must stay pending");
    assert!(first_segment.values().is_empty());
    let completed_graphic = adapter
        .accept_message(
            &segmented_message(&fixture, 1, &graphical[..split]),
            contributor(9, 2_000_000),
        )
        .expect("the second graphical segment must complete the file");
    assert!(completed_graphic.values().is_empty());

    let source_7 = adapter
        .accept_message(
            &message(&fixture, decode_hex(&fixture.text_payload_hex)),
            contributor(7, 3_000_000),
        )
        .expect("the first matching text record must release one contributor");
    assert_eq!(source_7.values().len(), 1);
    assert_advisory_source(&source_7.values()[0], 7);

    let source_9 = adapter
        .accept_message(
            &message(&fixture, decode_hex(&fixture.text_payload_hex)),
            contributor(9, 4_000_000),
        )
        .expect("the second matching text record must release its contributor");
    assert_eq!(source_9.values().len(), 1);
    assert_advisory_source(&source_9.values()[0], 9);

    let mut store = WeatherStore::new(StoreConfig::default(), ProducerInstanceId::new(77))
        .expect("the default store must be valid");
    accept_all(&mut store, &source_7, 5_000_000);
    accept_all(&mut store, &source_9, 5_000_001);
}

fn accept_all(
    store: &mut WeatherStore,
    batch: &airmass_aero_link::WeatherIngressBatch,
    monotonic_micros: u64,
) {
    for ingress in batch.values() {
        let evaluation = EvaluationTime::new(
            valid_start(ingress),
            MonotonicTime::from_micros(monotonic_micros),
        );
        store
            .accept(ingress.clone(), evaluation)
            .expect("each emitted contributor must pass core validation");
    }
}

fn assert_advisory_source(ingress: &WeatherIngress, source_id: u32) {
    let advisory = advisory(ingress);
    let expected = format!("aero-link-source-{source_id}");
    let advisory_type = advisory
        .advisory_type()
        .as_present()
        .expect("the paired advisory must contain its type");
    let geometry = advisory
        .geometry()
        .as_present()
        .expect("the paired advisory must contain geometry");
    assert_eq!(advisory_type.provenance.source.id.as_str(), expected);
    assert_eq!(geometry.provenance.source.id.as_str(), expected);
}

fn valid_start(ingress: &WeatherIngress) -> UtcTime {
    let validity = advisory(ingress)
        .validity()
        .as_present()
        .expect("the paired advisory must contain validity")
        .value;
    match validity {
        AdvisoryValidity::Period(period) => period.start,
        AdvisoryValidity::Point(point) => point,
        _ => panic!("the paired advisory must use supported validity"),
    }
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
    product
        .typed_value()
        .as_present()
        .and_then(|field| field.value.advisories())
        .and_then(|values| values.first())
        .expect("the paired product must contain one advisory")
}

fn segmented_message(fixture: &PairFixture, apdu_number: u16, payload: &[u8]) -> FisBMessage {
    let mut value = message(fixture, payload.to_vec());
    value.flags.segmented = true;
    value.segmentation = Some(FisBSegmentation {
        product_file_id: 77,
        product_file_length: 2,
        apdu_number,
    });
    value
}

fn message(fixture: &PairFixture, payload: Vec<u8>) -> FisBMessage {
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

fn contributor(source_id: u32, time: u64) -> FisBContributor {
    FisBContributor {
        source_id,
        source_epoch: 1,
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
    serde_json::from_value(serde_json::json!({
        "id": id,
        "name": "SIGMET and Convective SIGMET",
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
