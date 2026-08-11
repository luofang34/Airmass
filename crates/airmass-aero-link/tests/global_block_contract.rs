//! Contract tests for ignored FIS-B Global Block products.

#![allow(clippy::expect_used)]

use aero_link::{
    FisBContributor, FisBDeliveryPath, FisBFlags, FisBMessage, FisBOrigin, FisBProduct,
    FisBSegmentation, FisBSourceTimeQuality, FisBTime,
};
use airmass_aero_link::AeroLinkAdapter;

const GLOBAL_BLOCK_PRODUCTS: [(u16, &str); 8] = [
    (63, "Regional NEXRAD precipitation"),
    (64, "CONUS NEXRAD precipitation"),
    (70, "Icing forecast potential, low altitudes"),
    (71, "Icing forecast potential, high altitudes"),
    (84, "Cloud tops"),
    (90, "Turbulence forecast, low altitudes"),
    (91, "Turbulence forecast, high altitudes"),
    (103, "Lightning"),
];

#[test]
fn each_global_block_product_is_ignored_and_counted() {
    let mut adapter = AeroLinkAdapter::default();
    assert_eq!(adapter.ignored_global_block_products(), 0);

    for (id, name) in GLOBAL_BLOCK_PRODUCTS {
        let batch = adapter
            .accept_message(&message(id, name, &[1, 2, 3]), contributor(u64::from(id)))
            .expect("a Global Block product must not fail the adapter");
        assert!(batch.values().is_empty(), "product {id}");
    }

    assert_eq!(adapter.ignored_global_block_products(), 8);
    assert_eq!(adapter.counters().completed_products, 8);
}

#[test]
fn segmented_global_block_counts_once_when_complete() {
    let mut adapter = AeroLinkAdapter::default();
    let pending = adapter
        .accept_message(&segmented_message(2, &[3, 4]), contributor(1_000_000))
        .expect("the second segment must stay pending");
    assert!(pending.values().is_empty());
    assert_eq!(adapter.ignored_global_block_products(), 0);

    let complete = adapter
        .accept_message(&segmented_message(1, &[1, 2]), contributor(1_000_100))
        .expect("the first segment must complete the product");
    assert!(complete.values().is_empty());
    assert_eq!(adapter.ignored_global_block_products(), 1);
    assert_eq!(adapter.counters().completed_products, 1);
}

fn segmented_message(apdu_number: u16, payload: &[u8]) -> FisBMessage {
    let mut message = message(63, GLOBAL_BLOCK_PRODUCTS[0].1, payload);
    message.flags.segmented = true;
    message.segmentation = Some(FisBSegmentation {
        product_file_id: 21,
        product_file_length: 2,
        apdu_number,
    });
    message
}

fn message(id: u16, name: &str, payload: &[u8]) -> FisBMessage {
    FisBMessage {
        flags: FisBFlags {
            a: false,
            g: false,
            p: false,
            segmented: false,
        },
        product: product(id, name),
        time: FisBTime {
            month: None,
            day: None,
            hour: 12,
            minute: 30,
            second: None,
        },
        segmentation: None,
        payload: payload.to_vec(),
        text_reports: Vec::new(),
    }
}

fn product(id: u16, name: &str) -> FisBProduct {
    serde_json::from_value(serde_json::json!({
        "id": id,
        "name": name,
        "format": "global_block",
        "status": "standard",
    }))
    .expect("the fixture product metadata must match aero-link")
}

fn contributor(time: u64) -> FisBContributor {
    FisBContributor {
        source_id: 7,
        source_epoch: 1,
        origin: FisBOrigin::UatGroundStation,
        delivery_path: FisBDeliveryPath::LocalReceiver,
        first_received_at_micros: time,
        last_received_at_micros: time,
        station_identity: None,
        source_time_quality: FisBSourceTimeQuality::LocalIngress,
        uncertainty_micros: Some(0),
    }
}
