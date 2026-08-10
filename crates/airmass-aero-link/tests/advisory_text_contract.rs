//! Product-specific TWGO text contract tests.

#![allow(clippy::expect_used, clippy::panic)]

use aero_link::{
    FisBContributor, FisBDeliveryPath, FisBFlags, FisBMessage, FisBOrigin, FisBSourceTimeQuality,
    FisBTime, Product8RecordData, decode_product_8,
};
use airmass_aero_link::AeroLinkAdapter;
use serde::Deserialize;

const CWA_TEXT: &str = include_str!("../test-data/advisories/faa-cwa-text.json");

#[derive(Clone, Copy, Debug, Deserialize)]
struct FixtureTime {
    month: u8,
    day: u8,
    hour: u8,
    minute: u8,
}

#[derive(Debug, Deserialize)]
struct TextFixture {
    source_url: String,
    product_id: u16,
    report_year: u8,
    report_number: u16,
    issue_time: FixtureTime,
    raw_text: String,
}

#[test]
fn faa_product_15_active_text_is_non_fatal_without_graphical_geometry() {
    let fixture: TextFixture =
        serde_json::from_str(CWA_TEXT).expect("the CWA fixture must be valid");
    assert!(fixture.source_url.contains("faa.gov"));
    let payload = twgo_text_file(
        fixture.report_number,
        fixture.report_year,
        &fixture.raw_text,
    );
    let decoded = decode_product_8(&payload).expect("the Product 15 text must decode");
    let Product8RecordData::Text { text, .. } = &decoded.records[0].data else {
        panic!("the Product 15 fixture must contain text");
    };
    assert!(text.contains("CWA 101 VALID UNTIL"));
    let mut adapter = AeroLinkAdapter::default();
    let batch = adapter
        .accept_message(
            &message(fixture.product_id, fixture.issue_time, payload),
            contributor(1_000_000),
        )
        .expect("the Product 15 text must not fail mapping");
    assert!(batch.values().is_empty());
}

fn message(id: u16, issue: FixtureTime, payload: Vec<u8>) -> FisBMessage {
    FisBMessage {
        flags: FisBFlags {
            a: false,
            g: false,
            p: false,
            segmented: false,
        },
        product: serde_json::from_value(serde_json::json!({
            "id": id,
            "name": "Center weather advisory",
            "format": "text_or_graphic",
            "status": "standard",
        }))
        .expect("the Product 15 metadata must match aero-link"),
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

fn contributor(time: u64) -> FisBContributor {
    FisBContributor {
        source_id: 7,
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

fn twgo_text_file(report_number: u16, report_year: u8, text: &str) -> Vec<u8> {
    let encoded = encode_dlac(&text.replace('\n', " "));
    let length = encoded.len() + 5;
    let mut payload = vec![0x21, 0x10, 0, 0, 0, 0xff];
    payload.extend_from_slice(
        &u16::try_from(length)
            .expect("the text record length must fit")
            .to_be_bytes(),
    );
    payload.push(u8::try_from(report_number >> 6).expect("the report number must fit"));
    payload.push(u8::try_from((report_number & 0x3f) << 2).expect("the report number must fit"));
    payload.push((report_year << 3) | 4);
    payload.extend_from_slice(&encoded);
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
