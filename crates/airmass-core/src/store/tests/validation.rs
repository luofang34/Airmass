use alloc::vec;

use crate::{
    AdvisoryAltitude, AdvisoryAltitudeBand, AdvisoryAltitudeReference, AdvisoryGeometry,
    AdvisoryPolygon, AdvisoryPosition, AdvisoryRing, AdvisoryValidity, FieldState,
    ProducerInstanceId, SourceProductRevision, StoreConfig, StoreError, TextWeatherReport,
    UtcInterval, UtcTime, WeatherAdvisory, WeatherAdvisoryType, WeatherIngress, WeatherPayload,
    WeatherProduct, WeatherProductId, WeatherReportType, WeatherStore, WeatherValue,
};

use super::{PRODUCT, absent, at, field};

#[test]
fn field_evidence_must_name_the_event_source() {
    let payload = field(
        WeatherPayload::new("application/octet-stream", vec![1]),
        "alpha",
        1,
        10,
    );
    let product = WeatherProduct::new(
        payload,
        absent(),
        FieldState::present(field(UtcTime::from_unix_millis(1), "beta", 1, 10)),
        absent(),
        absent(),
        absent(),
    );
    let ingress = ingress(product);
    let error = store()
        .accept(ingress, at(10, 10))
        .expect_err("mixed field sources must fail");
    assert!(matches!(error, StoreError::FieldSourceMismatch { .. }));
}

#[test]
fn typed_report_field_evidence_must_name_the_event_source() {
    let report = TextWeatherReport::new(
        FieldState::present(field(WeatherReportType::Metar, "beta", 1, 10)),
        absent(),
        absent(),
        field("10SM CLR".into(), "alpha", 1, 10),
        absent(),
    );
    let typed_value = FieldState::present(field(
        WeatherValue::TextReports(vec![report]),
        "alpha",
        1,
        10,
    ));
    let product = WeatherProduct::new(
        field(
            WeatherPayload::new("application/octet-stream", vec![1]),
            "alpha",
            1,
            10,
        ),
        typed_value,
        absent(),
        absent(),
        absent(),
        absent(),
    );
    let error = store()
        .accept(ingress(product), at(10, 10))
        .expect_err("mixed report sources must fail");
    assert!(matches!(
        error,
        StoreError::FieldSourceMismatch {
            field: "report.type",
            ..
        }
    ));
}

#[test]
fn typed_advisory_field_evidence_must_name_the_event_source() {
    let advisory = WeatherAdvisory::new(
        absent(),
        FieldState::present(field(WeatherAdvisoryType::Sigmet, "beta", 1, 10)),
        absent(),
        absent(),
        absent(),
        absent(),
    );
    let typed_value = FieldState::present(field(
        WeatherValue::Advisories(vec![advisory]),
        "alpha",
        1,
        10,
    ));
    let product = WeatherProduct::new(
        field(
            WeatherPayload::new("application/octet-stream", vec![1]),
            "alpha",
            1,
            10,
        ),
        typed_value,
        absent(),
        absent(),
        absent(),
        absent(),
    );

    let error = store()
        .accept(ingress(product), at(10, 10))
        .expect_err("mixed advisory sources must fail");

    assert!(matches!(
        error,
        StoreError::FieldSourceMismatch {
            field: "advisory.type",
            ..
        }
    ));
}

#[test]
fn advisory_and_product_validity_must_match() {
    let advisory_validity =
        UtcInterval::new(UtcTime::from_unix_millis(10), UtcTime::from_unix_millis(20));
    let product_validity =
        UtcInterval::new(UtcTime::from_unix_millis(10), UtcTime::from_unix_millis(30));
    let advisory = WeatherAdvisory::new(
        absent(),
        absent(),
        FieldState::present(field(
            AdvisoryValidity::Period(advisory_validity),
            "alpha",
            1,
            10,
        )),
        absent(),
        absent(),
        absent(),
    );
    let product = WeatherProduct::new(
        field(
            WeatherPayload::new("application/octet-stream", vec![1]),
            "alpha",
            1,
            10,
        ),
        FieldState::present(field(
            WeatherValue::Advisories(vec![advisory]),
            "alpha",
            1,
            10,
        )),
        absent(),
        absent(),
        FieldState::present(field(product_validity, "alpha", 1, 10)),
        absent(),
    );

    let error = store()
        .accept(ingress(product), at(10, 10))
        .expect_err("two advisory validity intervals must fail");

    assert!(matches!(error, StoreError::AdvisoryValidityMismatch { .. }));
}

#[test]
fn malformed_advisory_structure_cannot_deserialize() {
    let valid =
        serde_json::to_value(structural_ingress()).expect("the valid ingress must serialize");
    for (pointer, replacement) in [
        (
            "/event/value/typed_value/field/value/value/0/geometry/field/value/value/rings/0/positions/0/latitude_e6",
            serde_json::json!(90_000_001),
        ),
        (
            "/event/value/typed_value/field/value/value/0/geometry/field/value/value/rings/0/positions/3/longitude_e6",
            serde_json::json!(1),
        ),
        (
            "/event/value/typed_value/field/value/value/0/altitude_band/field/value/base/feet",
            serde_json::json!(300),
        ),
        (
            "/event/value/typed_value/field/value/value/0/validity/field/value/value/end",
            serde_json::json!(10),
        ),
    ] {
        let mut value = valid.clone();
        let field = value
            .pointer_mut(pointer)
            .expect("the serialized advisory path must exist");
        *field = replacement;
        let decoded = serde_json::from_value::<WeatherIngress>(value);
        assert!(decoded.is_err(), "malformed advisory JSON must fail");
    }
}

fn structural_ingress() -> WeatherIngress {
    let interval = UtcInterval::new(UtcTime::from_unix_millis(10), UtcTime::from_unix_millis(20));
    let positions = vec![
        AdvisoryPosition::new(0, 0).expect("position must be valid"),
        AdvisoryPosition::new(1_000_000, 0).expect("position must be valid"),
        AdvisoryPosition::new(0, 1_000_000).expect("position must be valid"),
        AdvisoryPosition::new(0, 0).expect("position must be valid"),
    ];
    let ring = AdvisoryRing::new(positions).expect("ring must be valid");
    let polygon = AdvisoryPolygon::new(vec![ring]).expect("polygon must be valid");
    let band = AdvisoryAltitudeBand::new(
        AdvisoryAltitude::new(100, AdvisoryAltitudeReference::MeanSeaLevel),
        AdvisoryAltitude::new(200, AdvisoryAltitudeReference::MeanSeaLevel),
    )
    .expect("band must be valid");
    let advisory = WeatherAdvisory::new(
        absent(),
        absent(),
        FieldState::present(field(AdvisoryValidity::Period(interval), "alpha", 1, 10)),
        FieldState::present(field(AdvisoryGeometry::Polygon(polygon), "alpha", 1, 10)),
        absent(),
        FieldState::present(field(band, "alpha", 1, 10)),
    );
    ingress(WeatherProduct::new(
        field(
            WeatherPayload::new("application/octet-stream", vec![1]),
            "alpha",
            1,
            10,
        ),
        FieldState::present(field(
            WeatherValue::Advisories(vec![advisory]),
            "alpha",
            1,
            10,
        )),
        absent(),
        FieldState::present(field(interval.start, "alpha", 1, 10)),
        FieldState::present(field(interval, "alpha", 1, 10)),
        absent(),
    ))
}

fn ingress(product: WeatherProduct) -> WeatherIngress {
    WeatherIngress::product(
        WeatherProductId::new(PRODUCT),
        SourceProductRevision::new(1),
        product,
    )
}

fn store() -> WeatherStore {
    WeatherStore::new(StoreConfig::default(), ProducerInstanceId::new(1))
        .expect("configuration must be valid")
}
