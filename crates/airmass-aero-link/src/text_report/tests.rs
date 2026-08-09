#![allow(clippy::expect_used, clippy::panic)]

use aero_link::TextReport;
use airmass_core::{
    AbsenceEvidence, AbsenceReason, DeliveryPath, FieldState, FlightCategory, IngressTimeBasis,
    MonotonicTime, ProductProvenance, QualityEvidence, SourceEpoch, SourceId, SourceRef,
    SourceTime, TimeEvidence, TimeUncertainty, WeatherOrigin, WeatherReportType,
};

use super::{SourceEvidence, map_report, metar, parse_report_time};

#[test]
fn maps_each_supported_report_designator() {
    for (designator, expected) in [
        ("METAR", WeatherReportType::Metar),
        ("SPECI", WeatherReportType::Speci),
        ("TAF", WeatherReportType::Taf),
        ("PIREP", WeatherReportType::Pirep),
        ("WINDS", WeatherReportType::WindsAndTemperaturesAloft),
    ] {
        let report = map_report(report(designator, "10SM CLR"), &evidence());
        assert_eq!(
            report.report_type().as_present().map(|field| field.value),
            Some(expected),
            "{designator}"
        );
    }
}

#[test]
fn maps_the_two_token_amended_taf_layout() {
    let report = map_report(
        TextReport {
            report_type: Some("TAF".into()),
            location: Some("AMD".into()),
            issue_time: Some("KJFK".into()),
            text: "061651Z 0618/0724 18005KT P6SM SKC".into(),
        },
        &evidence(),
    );
    assert_eq!(
        report.report_type().as_present().map(|field| field.value),
        Some(WeatherReportType::AmendedTaf)
    );
    assert_eq!(
        report
            .station_id()
            .as_present()
            .map(|field| field.value.as_str()),
        Some("KJFK")
    );
    let time = report
        .report_time()
        .as_present()
        .expect("the amended TAF time must be valid");
    assert_eq!(
        (
            time.value.day_of_month(),
            time.value.hour(),
            time.value.minute()
        ),
        (6, 16, 51)
    );
    assert_eq!(report.text().value, "061651Z 0618/0724 18005KT P6SM SKC");
}

#[test]
fn keeps_the_source_text_unchanged() {
    let source = "AUTO 18005KT 5SM  BKN030\nRMK EXACT SPACING";
    let report = map_report(report("METAR", source), &evidence());
    assert_eq!(report.text().value, source);
}

#[test]
fn derives_metar_and_speci_flight_categories() {
    for (designator, text, expected) in [
        ("METAR", "18005KT 10SM BKN040", FlightCategory::Vfr),
        ("METAR", "18005KT 5SM BKN040", FlightCategory::Mvfr),
        ("SPECI", "18005KT 10SM OVC008", FlightCategory::Ifr),
        ("SPECI", "18005KT M1/4SM VV002", FlightCategory::Lifr),
    ] {
        let report = map_report(report(designator, text), &evidence());
        assert_eq!(
            report
                .flight_category()
                .as_present()
                .map(|field| field.value),
            Some(expected),
            "{designator} {text}"
        );
    }
}

#[test]
fn parses_mixed_visibility_and_uses_the_lowest_ceiling() {
    assert_eq!(
        metar::flight_category("18005KT 1 1/2SM BKN020 OVC004"),
        Some(FlightCategory::Lifr)
    );
}

#[test]
fn incomplete_ceiling_evidence_does_not_make_a_category() {
    assert_eq!(metar::flight_category("18005KT 10SM"), None);
    assert_eq!(metar::flight_category("18005KT 10SM BKN///"), None);
}

#[test]
fn remarks_do_not_change_the_flight_category() {
    assert_eq!(
        metar::flight_category("18005KT 10SM CLR RMK BKN001"),
        Some(FlightCategory::Vfr)
    );
}

#[test]
fn a_report_without_station_or_time_stays_present() {
    let report = map_report(
        TextReport {
            report_type: Some("TAF".into()),
            location: None,
            issue_time: None,
            text: "SOURCE TEXT".into(),
        },
        &evidence(),
    );
    assert_eq!(
        absence_reason(report.station_id()),
        AbsenceReason::NotObserved
    );
    assert_eq!(
        absence_reason(report.report_time()),
        AbsenceReason::NotObserved
    );
    assert_eq!(report.text().value, "SOURCE TEXT");
}

#[test]
fn an_invalid_time_is_rejected_without_rejecting_the_report() {
    let mut source = report("PIREP", "SOURCE TEXT");
    source.issue_time = Some("322460Z".into());
    let report = map_report(source, &evidence());
    assert_eq!(
        absence_reason(report.report_time()),
        AbsenceReason::Rejected
    );
    assert_eq!(report.text().value, "SOURCE TEXT");
}

#[test]
fn unsupported_report_type_keeps_other_source_fields() {
    let report = map_report(report("FUTURE", "SOURCE TEXT"), &evidence());
    assert_eq!(
        absence_reason(report.report_type()),
        AbsenceReason::Unsupported
    );
    assert_eq!(report.text().value, "SOURCE TEXT");
}

#[test]
fn parses_a_valid_report_time() {
    let time = parse_report_time("250000Z").expect("the report time must be valid");
    assert_eq!(
        (time.day_of_month(), time.hour(), time.minute()),
        (25, 0, 0)
    );
}

fn report(designator: &str, text: &str) -> TextReport {
    TextReport {
        report_type: Some(designator.into()),
        location: Some("KJFK".into()),
        issue_time: Some("061651Z".into()),
        text: text.into(),
    }
}

fn evidence() -> SourceEvidence {
    SourceEvidence::new(
        TimeEvidence::with_ingress_evidence(
            MonotonicTime::from_micros(10),
            IngressTimeBasis::LocalIngress,
            TimeUncertainty::Bounded(0),
            SourceTime::Unknown,
        ),
        QualityEvidence::Unavailable,
        ProductProvenance::new(
            SourceRef::new(SourceId::new("test-source"), SourceEpoch::new(1)),
            WeatherOrigin::Simulation,
            DeliveryPath::new("test"),
            None,
        ),
    )
}

fn absence_reason<T>(field: &FieldState<T>) -> AbsenceReason {
    match field {
        FieldState::Absent(absence) => {
            assert!(matches!(absence.evidence(), AbsenceEvidence::Source { .. }));
            absence.reason()
        }
        FieldState::Present(_) => panic!("the field must be absent"),
    }
}
