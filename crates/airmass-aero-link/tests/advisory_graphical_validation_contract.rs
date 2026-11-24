//! Product-specific TWGO graphical validation contract tests.

// The test exemptions in `clippy.toml` apply only below `cfg(test)`, and the
// helpers in this file are not `#[test]` functions.
#![cfg(test)]

use aero_link::{
    FisBContributor, FisBDeliveryPath, FisBFlags, FisBMessage, FisBOrigin, FisBProduct,
    FisBSourceTimeQuality, FisBTime,
};
use airmass_aero_link::{AdapterError, AdvisoryDecodeError, AeroLinkAdapter, MappingError};
use serde::Deserialize;

const G_AIRMET: &str = include_str!("../test-data/advisories/recorded-g-airmet.json");
const RECORD_OFFSET: usize = 6;

#[derive(Clone, Copy, Debug, Deserialize)]
struct FixtureTime {
    month: u8,
    day: u8,
    hour: u8,
    minute: u8,
}

#[test]
fn products_14_and_15_reject_the_object_label_flag() {
    let (payload, issue) = g_airmet_payload();
    for product_id in [14, 15] {
        let mut invalid = payload.clone();
        invalid[RECORD_OFFSET + 4] |= 1;
        assert_unsupported(product_id, issue, invalid, "object label flag", 1);
    }
}

#[test]
fn non_g_airmet_products_reject_the_object_qualifier_flag() {
    let (payload, issue) = g_airmet_payload();
    for product_id in [11, 15] {
        let mut invalid = payload.clone();
        let object = object_offset(&invalid);
        invalid[object] &= !0x80;
        invalid[object] |= 0x40;
        assert_unsupported(product_id, issue, invalid, "object qualifier flag", 1);
    }
}

#[test]
fn product_14_rejects_a_qualifier_for_other_object_elements() {
    let (mut payload, issue) = g_airmet_payload();
    let object = object_offset(&payload);
    payload[object] = (payload[object] & !0x1f) | 1;
    payload[object] |= 0x40;
    assert_unsupported(14, issue, payload, "object qualifier flag", 1);
}

#[test]
fn product_14_rejects_object_elements_outside_one_through_seven() {
    let (payload, issue) = g_airmet_payload();
    for value in [0, 8, 31] {
        let mut invalid = payload.clone();
        let object = object_offset(&invalid);
        invalid[object] = (invalid[object] & !0x1f) | value;
        assert_unsupported(14, issue, invalid, "object element", value);
    }
}

#[test]
fn product_15_rejects_graphical_cancellation_status() {
    let (mut payload, issue) = product_15_payload();
    set_applicability(&mut payload, [8, 9, 15, 0], [8, 9, 16, 0]);
    let object = object_offset(&payload);
    payload[object + 1] = (payload[object + 1] & 0xf0) | 13;
    assert_unsupported(15, issue, payload, "object status", 13);
}

#[test]
fn product_15_rejects_validity_longer_than_two_hours() {
    let (mut payload, issue) = product_15_payload();
    set_applicability(&mut payload, [8, 9, 15, 0], [8, 9, 17, 1]);
    assert_advisory_error(
        issue,
        payload,
        |source| matches!(source, AdvisoryDecodeError::CwaDurationTooLong { .. }),
        "a CWA validity longer than two hours must fail mapping",
    );
}

#[test]
fn product_15_rejects_a_start_more_than_two_hours_after_issue() {
    let (mut payload, issue) = product_15_payload();
    set_applicability(&mut payload, [8, 9, 16, 46], [8, 9, 17, 46]);
    assert_advisory_error(
        issue,
        payload,
        |source| matches!(source, AdvisoryDecodeError::CwaStartTooLate { .. }),
        "a CWA start more than two hours after issue must fail mapping",
    );
}

fn assert_unsupported(
    product_id: u16,
    issue: FixtureTime,
    payload: Vec<u8>,
    field: &'static str,
    value: u8,
) {
    let mut adapter = AeroLinkAdapter::default();
    let error = adapter
        .accept_message(&message(product_id, issue, payload), contributor(1_000_000))
        .expect_err("the invalid graphical field must fail mapping");
    let AdapterError::Mapping(MappingError::Advisory {
        source:
            AdvisoryDecodeError::UnsupportedGraphicalField {
                field: actual_field,
                value: actual_value,
            },
        ..
    }) = error
    else {
        panic!("the adapter must report the unsupported graphical field");
    };
    assert_eq!(actual_field, field);
    assert_eq!(actual_value, value);
}

fn assert_advisory_error(
    issue: FixtureTime,
    payload: Vec<u8>,
    matches_error: impl FnOnce(&AdvisoryDecodeError) -> bool,
    failure: &'static str,
) {
    let mut adapter = AeroLinkAdapter::default();
    let error = adapter
        .accept_message(&message(15, issue, payload), contributor(1_000_000))
        .expect_err(failure);
    let AdapterError::Mapping(MappingError::Advisory { source, .. }) = error else {
        panic!("the adapter must report an advisory decode error");
    };
    assert!(matches_error(&source));
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

fn product_15_payload() -> (Vec<u8>, FixtureTime) {
    let (mut payload, issue) = g_airmet_payload();
    let object = object_offset(&payload);
    payload[object] &= !0x80;
    (payload, issue)
}

fn object_offset(payload: &[u8]) -> usize {
    let label_bytes = if payload[RECORD_OFFSET + 4] & 1 == 0 {
        2
    } else {
        9
    };
    RECORD_OFFSET + 5 + label_bytes
}

fn set_applicability(payload: &mut [u8], start: [u8; 4], end: [u8; 4]) {
    let object = object_offset(payload);
    let qualifier_bytes = if payload[object] & 0x40 == 0 { 0 } else { 3 };
    let validity = object + 2 + qualifier_bytes + 2;
    payload[validity..validity + 4].copy_from_slice(&start);
    payload[validity + 4..validity + 8].copy_from_slice(&end);
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

fn identified_product(id: u16) -> FisBProduct {
    let name = match id {
        11 => "AIRMET",
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
