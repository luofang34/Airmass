//! Contract tests for FIS-B messages from `aero-link`.

#![allow(clippy::panic)]

use aero_link::{CorrectedUatFrame, ReceptionEvent, UatDecoder, UatFrameClass};
use airmass_aero_link::AeroLinkAdapter;

const UNKNOWN_PRODUCT_ID: u16 = 700;
const UNKNOWN_PAYLOAD: [u8; 3] = [0xca, 0xfe, 0x01];

#[test]
fn traffic_downlink_in_a_mixed_stream_has_no_weather_product_or_error() {
    let traffic = traffic_event();
    assert_eq!(traffic.fis_b_messages().count(), 0);

    let events = [traffic, uplink_event()];
    let product_ids: Vec<_> = events
        .iter()
        .flat_map(ReceptionEvent::fis_b_messages)
        .map(|message| message.product.id)
        .collect();

    assert_eq!(product_ids, [413, UNKNOWN_PRODUCT_ID]);
}

#[test]
fn unknown_product_keeps_its_payload_bytes() {
    let event = uplink_event();
    let Some(product) = event
        .fis_b_messages()
        .find(|message| message.product.id == UNKNOWN_PRODUCT_ID)
    else {
        panic!("the uplink must contain the unknown product");
    };

    assert_eq!(product.payload.as_slice(), UNKNOWN_PAYLOAD);
}

#[test]
fn information_frame_projection_admits_only_weather() {
    let event = uplink_event();
    let mut adapter = AeroLinkAdapter::default();
    let mut weather = Vec::new();
    for projection in event.information_frames() {
        let batch = adapter.accept_projection(projection);
        let Ok(batch) = batch else {
            panic!("the recorded projection must adapt without an error");
        };
        weather.extend(batch.into_values());
    }

    assert_eq!(weather.len(), 1);
    assert_eq!(weather[0].product_id().as_str(), "fisb-product-413");
}

fn traffic_event() -> ReceptionEvent {
    let bytes = [
        0x00, 0x7e, 0x69, 0x87, 0xd2, 0xd7, 0x42, 0x38, 0xc1, 0x45, 0x38, 0x55, 0xf8, 0x99, 0x80,
        0xc7, 0x52, 0x4f,
    ];
    let frame = CorrectedUatFrame::try_from_boxed_bytes(
        UatFrameClass::BasicDownlink,
        bytes.to_vec().into_boxed_slice(),
    );
    let Ok(frame) = frame else {
        panic!("the traffic fixture must have the correct length");
    };
    let reception = UatDecoder::default().decode_corrected(frame, -20, 1, 0);
    let Ok(reception) = reception else {
        panic!("the traffic fixture must decode without an error");
    };
    ReceptionEvent::uat978(1, reception)
}

fn uplink_event() -> ReceptionEvent {
    let mut bytes = vec![0; UatFrameClass::GroundUplink.corrected_bytes()];
    bytes[6] = 0x20;
    bytes[8..14].copy_from_slice(&[2, 0, 0x06, 0x74, 0x41, 0x90]);
    bytes[14..23].copy_from_slice(&[3, 0x80, 0x0a, 0xf0, 0, 0, 0xca, 0xfe, 0x01]);
    let frame = CorrectedUatFrame::try_from_boxed_bytes(
        UatFrameClass::GroundUplink,
        bytes.into_boxed_slice(),
    );
    let Ok(frame) = frame else {
        panic!("the uplink fixture must have the correct length");
    };
    let reception = UatDecoder::default().decode_corrected(frame, -30, 2, 0);
    let Ok(reception) = reception else {
        panic!("the uplink fixture must decode without an error");
    };
    ReceptionEvent::uat978(2, reception)
}
