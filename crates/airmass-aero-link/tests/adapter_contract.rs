//! Public contract tests for the `aero-link` weather adapter.

#![allow(clippy::expect_used, clippy::panic)]

use aero_link::{
    CorrectedUatFrame, FisBAssemblerConfig, FisBContributor, FisBDeliveryPath, FisBFlags,
    FisBMessage, FisBOrigin, FisBOutputLimits, FisBProduct, FisBProvenanceLimits, FisBSegmentation,
    FisBSourceTimeQuality, FisBTime, ReceptionEvent, UatDecoder, UatFrameClass,
};
use airmass_aero_link::{AdapterError, AeroLinkAdapter};
use airmass_core::{
    AbsenceEvidence, AbsenceReason, FieldState, FlightCategory, IngressTimeBasis, SourceEpoch,
    SourceProductRevision, TimeUncertainty, TimedField, WeatherIngressEvent, WeatherOrigin,
    WeatherProduct, WeatherReportType,
};

const CAPTURED_UPLINK: &str = include_str!("../test-data/uat/uplink-captured-corrected.hex");
const G_AIRMET_FIXTURE: &str = include_str!("../test-data/advisories/recorded-g-airmet.json");
const CAPTURED_WINDS_TEXT: &str = concat!(
    "FT                       12000       18000   24000   30000    34000  39000                         \n",
    "                        3522+00 3631-13 0141-27 035046 046656 365258",
);

const MIXED_PRODUCT_8: &[u8] = &[
    33, 32, 45, 48, 203, 0, 0, 18, 1, 144, 212, 45, 48, 203, 129, 37, 217, 131, 46, 82, 128, 51,
    19, 16, 0, 48, 1, 148, 212, 24, 148, 237, 10, 12, 178, 195, 6, 160, 104, 208, 108, 129, 162,
    152, 128, 123, 65, 37, 35, 69, 82, 4, 18, 60, 69, 67, 82, 5, 80, 16, 21, 5, 78, 5, 78, 5, 96,
    73, 48, 16, 140, 20,
];
const METAR_PAYLOAD: &[u8] = &[
    52, 85, 1, 74, 2, 202, 24, 184, 48, 219, 29, 181, 197, 168, 49, 227, 12, 53, 45, 72, 53, 76,
    216, 2, 44, 236, 51, 194, 12, 176, 191, 28, 32, 7, 46, 121, 200,
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
    for id in [63_u16, 64, 70, 71, 84, 90, 91, 103, 413] {
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
    let fixture: serde_json::Value =
        serde_json::from_str(G_AIRMET_FIXTURE).expect("the advisory fixture must be valid");
    let payload = decode_hex(
        fixture["twgo_payload_hex"]
            .as_str()
            .expect("the advisory fixture must contain hex"),
    );
    let batch = adapter
        .accept_message(&message(14, &payload), contributor(7, 1, 2_000_014))
        .expect("the G-AIRMET product must map");
    assert_eq!(batch.values().len(), 1);
    assert_eq!(
        batch.values()[0].product_id().as_str(),
        "fisb-advisory-14-26-321"
    );
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
fn global_block_has_no_typed_value_and_no_error() {
    let mut adapter = AeroLinkAdapter::default();
    let batch = adapter
        .accept_message(&message(63, &[1, 2, 3]), contributor(7, 1, 100))
        .expect("the Global Block product must map without an error");
    let product = weather_product(&batch.values()[0]);
    let FieldState::Absent(absence) = product.typed_value() else {
        panic!("a Global Block product must not have a text value");
    };
    assert_eq!(absence.reason(), AbsenceReason::Unsupported);
    assert!(matches!(absence.evidence(), AbsenceEvidence::Source { .. }));
}

#[test]
fn product_413_maps_a_metar_with_its_flight_category() {
    let mut adapter = AeroLinkAdapter::default();
    let batch = adapter
        .accept_message(&message(413, METAR_PAYLOAD), contributor(7, 1, 100))
        .expect("the METAR product must map without an error");
    let product = weather_product(&batch.values()[0]);
    let reports = product
        .typed_value()
        .as_present()
        .and_then(|field| field.value.text_reports())
        .expect("Product 413 must have text reports");
    let report = reports.first().expect("the product must contain one METAR");
    assert_eq!(
        report.report_type().as_present().map(|field| field.value),
        Some(WeatherReportType::Metar)
    );
    assert_eq!(report.text().value, "18005KT 5SM BKN030 20/10 A2992");
    assert_eq!(
        report
            .flight_category()
            .as_present()
            .map(|field| field.value),
        Some(FlightCategory::Mvfr)
    );
    let encoded =
        serde_json::to_vec(&batch.values()[0]).expect("the typed ingress record must serialize");
    let decoded: airmass_core::WeatherIngress =
        serde_json::from_slice(&encoded).expect("the typed ingress record must deserialize");
    assert_eq!(decoded, batch.values()[0]);
}

#[test]
fn live_capture_maps_the_complete_winds_report_with_field_evidence() {
    let product = product_from_live_capture();
    let typed = product
        .typed_value()
        .as_present()
        .expect("Product 413 must have a typed value");
    let reports = typed
        .value
        .text_reports()
        .expect("Product 413 must contain text reports");
    assert_eq!(reports.len(), 1);
    let report = &reports[0];
    let report_type = report
        .report_type()
        .as_present()
        .expect("the captured report type must be present");
    assert_eq!(
        report_type.value,
        WeatherReportType::WindsAndTemperaturesAloft
    );
    let station = report
        .station_id()
        .as_present()
        .expect("the captured station must be present");
    assert_eq!(station.value.as_str(), "BCE");
    let time = report
        .report_time()
        .as_present()
        .expect("the captured report time must be valid");
    assert_eq!(
        (
            time.value.day_of_month(),
            time.value.hour(),
            time.value.minute()
        ),
        (25, 0, 0)
    );
    assert_eq!(report.text().value, CAPTURED_WINDS_TEXT);
    let FieldState::Absent(category) = report.flight_category() else {
        panic!("a winds report must not have a flight category");
    };
    assert_eq!(category.reason(), AbsenceReason::NotApplicable);
    assert!(matches!(
        category.evidence(),
        AbsenceEvidence::Source { .. }
    ));
    assert_same_evidence(typed, &product);
    assert_same_evidence(report_type, &product);
    assert_same_evidence(station, &product);
    assert_same_evidence(time, &product);
    assert_same_evidence(report.text(), &product);
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

fn product_from_live_capture() -> WeatherProduct {
    let bytes = decode_hex(CAPTURED_UPLINK.trim());
    let frame = CorrectedUatFrame::try_from_boxed_bytes(
        UatFrameClass::GroundUplink,
        bytes.into_boxed_slice(),
    )
    .expect("the captured frame must have the uplink length");
    let reception = UatDecoder::default()
        .decode_corrected(frame, -3, 9, 0)
        .expect("the captured uplink must decode");
    let event = ReceptionEvent::uat978(10, reception);
    let mut adapter = AeroLinkAdapter::default();
    let mut products = Vec::new();
    for projection in event.information_frames() {
        let batch = adapter
            .accept_projection(projection)
            .expect("the captured product must adapt");
        products.extend(
            batch
                .values()
                .iter()
                .map(|ingress| weather_product(ingress).clone()),
        );
    }
    assert_eq!(products.len(), 1);
    products
        .pop()
        .expect("the capture must contain Product 413")
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

fn assert_same_evidence<T>(field: &TimedField<T>, product: &WeatherProduct) {
    assert_eq!(field.time, product.payload().time);
    assert_eq!(field.quality, product.payload().quality);
    assert_eq!(field.provenance, product.payload().provenance);
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
