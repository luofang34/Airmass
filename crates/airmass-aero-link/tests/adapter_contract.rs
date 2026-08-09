//! Public contract tests for the `aero-link` weather adapter.

#![allow(clippy::expect_used, clippy::panic)]

use aero_link::{
    FisBAssemblerConfig, FisBContributor, FisBDeliveryPath, FisBFlags, FisBMessage, FisBOrigin,
    FisBOutputLimits, FisBProduct, FisBProvenanceLimits, FisBSegmentation, FisBSourceTimeQuality,
    FisBTime,
};
use airmass_aero_link::{AdapterError, AeroLinkAdapter};
use airmass_core::{
    AbsenceReason, FieldState, IngressTimeBasis, SourceEpoch, SourceProductRevision,
    TimeUncertainty, WeatherIngressEvent, WeatherOrigin,
};

const MIXED_PRODUCT_8: &[u8] = &[
    33, 32, 45, 48, 203, 0, 0, 18, 1, 144, 212, 45, 48, 203, 129, 37, 217, 131, 46, 82, 128, 51,
    19, 16, 0, 48, 1, 148, 212, 24, 148, 237, 10, 12, 178, 195, 6, 160, 104, 208, 108, 129, 162,
    152, 128, 123, 65, 37, 35, 69, 82, 4, 18, 60, 69, 67, 82, 5, 80, 16, 21, 5, 78, 5, 78, 5, 96,
    73, 48, 16, 140, 20,
];

#[test]
fn result_assembly_keeps_payload_and_each_contributor() {
    let mut adapter = AeroLinkAdapter::default();
    let second = segmented_message(2, &[0x33, 0x44]);
    let first = segmented_message(1, &[0x11, 0x22]);

    let pending = adapter
        .accept_message(&second, contributor(20, 1, 2_000_000))
        .expect("the first segment must be pending");
    assert!(pending.values().is_empty());

    let batch = adapter
        .accept_message(&first, contributor(7, 2, 2_000_100))
        .expect("the second segment must complete the product");
    assert_eq!(batch.values().len(), 2);
    assert!(!batch.contributors_overflowed());
    assert_eq!(adapter.counters().completed_products, 1);

    let first_product = weather_product(&batch.values()[0]);
    let second_product = weather_product(&batch.values()[1]);
    assert_eq!(
        first_product.payload().value.bytes,
        [0x11, 0x22, 0x33, 0x44]
    );
    assert_eq!(
        second_product.payload().value.bytes,
        [0x11, 0x22, 0x33, 0x44]
    );
    assert_eq!(
        first_product.payload().provenance.source.epoch,
        SourceEpoch::new(1)
    );
    assert_eq!(
        first_product.payload().provenance.source.id.as_str(),
        "aero-link-source-20"
    );
    assert_eq!(
        second_product.payload().provenance.source.epoch,
        SourceEpoch::new(2)
    );
    assert_eq!(
        second_product.payload().provenance.source.id.as_str(),
        "aero-link-source-7"
    );
    assert_eq!(
        second_product.payload().provenance.origin,
        WeatherOrigin::Other("uat-ground-station".into())
    );
    assert_eq!(
        second_product.payload().provenance.delivery_path.as_str(),
        "local-receiver"
    );
    let raw_product = second_product
        .payload()
        .provenance
        .raw_product
        .as_ref()
        .expect("the protocol identity must stay present");
    assert_eq!(raw_product.namespace, "aero-link/fis-b-exchange-v1");
    assert_eq!(
        raw_product.value,
        "product=63;file=Some(21);segments=2;record=None/None;time=None-NoneT12:30:None"
    );
    assert_eq!(
        batch.values()[1].source_product_revision(),
        SourceProductRevision::new((2_000_100_u64 << 10) | 21)
    );
    assert_eq!(
        second_product.payload().time.ingress_monotonic.as_micros(),
        2_000_100
    );
    assert_eq!(
        second_product.payload().time.ingress_basis,
        IngressTimeBasis::LocalIngress
    );
    assert_eq!(
        second_product.payload().time.ingress_uncertainty,
        TimeUncertainty::Bounded(0)
    );
}

#[test]
fn mapped_source_time_quality_and_uncertainty_stay_exact() {
    let mut adapter = AeroLinkAdapter::default();
    let mut source = contributor(7, 1, 500);
    source.source_time_quality = FisBSourceTimeQuality::MappedSourceTime;
    source.uncertainty_micros = Some(37);
    let batch = adapter
        .accept_message(&message(63, &[1]), source)
        .expect("the mapped source time must be valid");
    let time = weather_product(&batch.values()[0]).payload().time;
    assert_eq!(time.ingress_basis, IngressTimeBasis::MappedSourceTime);
    assert_eq!(time.ingress_uncertainty, TimeUncertainty::Bounded(37));
}

#[test]
fn contributor_overflow_stays_visible() {
    let config = FisBAssemblerConfig {
        provenance: FisBProvenanceLimits {
            max_contributors_per_product: 1,
        },
        ..FisBAssemblerConfig::default()
    };
    let mut adapter = AeroLinkAdapter::new(config).expect("one contributor must be valid");
    adapter
        .accept_message(&segmented_message(2, &[2]), contributor(20, 1, 100))
        .expect("the first segment must be pending");
    let batch = adapter
        .accept_message(&segmented_message(1, &[1]), contributor(7, 1, 101))
        .expect("the second segment must complete the product");
    assert_eq!(batch.values().len(), 1);
    assert!(batch.contributors_overflowed());
    assert_eq!(adapter.counters().contributor_overflows, 1);
}

#[test]
fn every_weather_product_class_maps_to_ingress() {
    let mut adapter = AeroLinkAdapter::default();
    for id in [11_u16, 12, 14, 15, 63, 64, 70, 71, 84, 90, 91, 103, 413] {
        let batch = adapter
            .accept_message(
                &message(id, &[id as u8]),
                contributor(7, 1, 1_000_000 + u64::from(id)),
            )
            .expect("a weather product must map");
        assert_eq!(batch.values().len(), 1, "product {id}");
        assert_eq!(
            batch.values()[0].product_id().as_str(),
            format!("fisb-product-{id}")
        );
    }
}

#[test]
fn mixed_product_8_emits_no_weather_and_no_error() {
    let mut adapter = AeroLinkAdapter::default();
    let batch = adapter
        .accept_message(&message(8, MIXED_PRODUCT_8), contributor(7, 2, 1_000_000))
        .expect("the mixed Product 8 fixture must decode");
    assert!(batch.values().is_empty());
}

#[test]
fn source_restart_keeps_the_new_epoch() {
    let mut adapter = AeroLinkAdapter::default();
    let first = adapter
        .accept_message(&message(63, &[1]), contributor(7, 1, 100))
        .expect("the first source epoch must map");
    let restarted = adapter
        .accept_message(&message(63, &[2]), contributor(7, 2, 200))
        .expect("the restarted source epoch must map");
    assert_eq!(
        weather_product(&first.values()[0])
            .payload()
            .provenance
            .source
            .epoch,
        SourceEpoch::new(1)
    );
    assert_eq!(
        weather_product(&restarted.values()[0])
            .payload()
            .provenance
            .source
            .epoch,
        SourceEpoch::new(2)
    );
}

#[test]
fn assembly_failure_is_not_an_empty_option() {
    let config = FisBAssemblerConfig {
        output: FisBOutputLimits {
            max_completed_product_bytes: 0,
            ..FisBOutputLimits::default()
        },
        ..FisBAssemblerConfig::default()
    };
    let mut adapter = AeroLinkAdapter::new(config).expect("the zero output limit is valid");
    let error = adapter
        .accept_message(&message(63, &[1]), contributor(7, 1, 100))
        .expect_err("the Result error must reach the adapter caller");
    assert!(matches!(error, AdapterError::Assembly(_)));
}

fn weather_product(ingress: &airmass_core::WeatherIngress) -> &airmass_core::WeatherProduct {
    let event = ingress.event().expect("the adapter schema must be current");
    let WeatherIngressEvent::Product(product) = event else {
        panic!("the adapter must emit a weather product");
    };
    let FieldState::Absent(issue_time) = product.issue_time() else {
        panic!("an APDU time without a complete civil date must stay absent");
    };
    assert_eq!(issue_time.reason(), AbsenceReason::Unsupported);
    product
}

fn segmented_message(apdu_number: u16, payload: &[u8]) -> FisBMessage {
    let mut message = message(63, payload);
    message.flags.segmented = true;
    message.segmentation = Some(FisBSegmentation {
        product_file_id: 21,
        product_file_length: 2,
        apdu_number,
    });
    message
}

fn message(id: u16, payload: &[u8]) -> FisBMessage {
    FisBMessage {
        flags: FisBFlags {
            a: false,
            g: false,
            p: false,
            segmented: false,
        },
        product: identified_product(id),
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

fn contributor(source_id: u32, source_epoch: u32, time: u64) -> FisBContributor {
    FisBContributor {
        source_id,
        source_epoch,
        origin: FisBOrigin::UatGroundStation,
        delivery_path: FisBDeliveryPath::LocalReceiver,
        first_received_at_micros: time,
        last_received_at_micros: time,
        station_identity: None,
        source_time_quality: FisBSourceTimeQuality::LocalIngress,
        uncertainty_micros: Some(0),
    }
}

fn identified_product(id: u16) -> FisBProduct {
    let (name, format) = match id {
        8 => ("NOTAM-D and FDC, including TFRs", "text_or_graphic"),
        11 => ("AIRMET", "text_or_graphic"),
        12 => ("SIGMET and Convective SIGMET", "text_or_graphic"),
        14 => ("G-AIRMET", "text_or_graphic"),
        15 => ("Center weather advisory", "text_or_graphic"),
        63 => ("Regional NEXRAD precipitation", "global_block"),
        64 => ("CONUS NEXRAD precipitation", "global_block"),
        70 => ("Icing forecast potential, low altitudes", "global_block"),
        71 => ("Icing forecast potential, high altitudes", "global_block"),
        84 => ("Cloud tops", "global_block"),
        90 => ("Turbulence forecast, low altitudes", "global_block"),
        91 => ("Turbulence forecast, high altitudes", "global_block"),
        103 => ("Lightning", "global_block"),
        413 => ("Generic textual product type 2", "dlac_text"),
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
