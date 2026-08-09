#![allow(clippy::expect_used)]

use alloc::vec;

use crate::{
    FieldState, ProducerInstanceId, SourceProductRevision, StoreConfig, StoreError,
    TextWeatherReport, UtcTime, WeatherIngress, WeatherPayload, WeatherProduct, WeatherProductId,
    WeatherReportType, WeatherStore, WeatherValue,
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
