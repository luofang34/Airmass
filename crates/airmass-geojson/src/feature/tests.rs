#![allow(clippy::expect_used, clippy::panic)]

use alloc::{string::String, vec::Vec};

use airmass_core::{
    AbsenceEvidence, AbsenceReason, AbsentField, DeliveryPath, DurationMillis, EvaluationTime,
    FieldState, FlightCategory, MonotonicTime, ProducerInstanceId, ProductProvenance,
    QualityEvidence, RawProductIdentity, SourceEpoch, SourceId, SourceProductRevision, SourceRef,
    SourceTime, StoreConfig, TextWeatherReport, TimeEvidence, TimedField, UtcTime, WeatherIngress,
    WeatherOrigin, WeatherPayload, WeatherProduct, WeatherProductId, WeatherReportTime,
    WeatherReportType, WeatherSnapshotHandle, WeatherStationId, WeatherStore, WeatherValue,
};
use serde::Deserialize;

use super::{FeatureDelta, FeatureRemovalReason, WeatherFeature, map_snapshot_transition};
use crate::Wgs84Position;

const RECORDED_METARS_JSON: &str = include_str!("../../test-data/recorded-metars.json");
const PRODUCT_ID: &str = "recorded-metar-set";

#[derive(Debug, Deserialize)]
struct RecordedMetarSet {
    source: String,
    records: Vec<RecordedMetar>,
}

#[derive(Clone, Debug, Deserialize)]
struct RecordedMetar {
    station_id: String,
    observation_day: u8,
    observation_hour: u8,
    observation_minute: u8,
    latitude_deg: f64,
    longitude_deg: f64,
    raw_text: String,
    report_text: String,
    flight_category: FlightCategory,
}

#[test]
fn recorded_metars_become_points_with_their_flight_categories() {
    let set = recorded_set();
    assert_eq!(
        set.source,
        "https://aviationweather.gov/data/cache/metars.cache.csv.gz"
    );
    let current = publish_product(&set.records, SourceProductRevision::new(1), 60_000);
    let resolver = recorded_resolver(&set.records);

    let deltas = map_snapshot_transition(None, current.envelope(), &resolver);

    assert_eq!(deltas.len(), set.records.len());
    for (delta, record) in deltas.iter().zip(&set.records) {
        let FeatureDelta::Upsert(WeatherFeature::TextReport(feature)) = delta else {
            panic!("a recorded METAR must produce one point upsert");
        };
        assert_eq!(
            feature
                .id()
                .station_id()
                .expect("a text feature must have a station")
                .as_str(),
            record.station_id
        );
        assert_eq!(feature.id().report_type(), Some(WeatherReportType::Metar));
        assert_eq!(feature.report().text().value, record.report_text);
        assert_eq!(
            feature
                .report()
                .flight_category()
                .as_present()
                .map(|field| field.value),
            Some(record.flight_category)
        );
        assert_eq!(feature.position().latitude_deg(), record.latitude_deg);
        assert_eq!(feature.position().longitude_deg(), record.longitude_deg);
        assert_eq!(feature.revisions().snapshot_revision().get(), 1);
        assert_eq!(feature.revisions().product_revision().get(), 1);
        assert_eq!(feature.revisions().source_product_revision().get(), 1);
    }
}

#[test]
fn an_expired_product_removes_each_prior_feature() {
    let set = recorded_set();
    let mut store = store();
    let accepted = store
        .accept(
            ingress(&set.records[..1], SourceProductRevision::new(1), 1),
            evaluation_time(100),
        )
        .expect("the recorded METAR must be valid")
        .into_publication()
        .expect("the first product must publish");
    let expired = store
        .advance_time(evaluation_time(1_101))
        .expect("time must advance")
        .into_publication()
        .expect("expiry must publish");
    let resolver = recorded_resolver(&set.records);

    let deltas = map_snapshot_transition(Some(accepted.envelope()), expired.envelope(), &resolver);

    assert!(matches!(
        deltas.as_slice(),
        [FeatureDelta::Remove {
            reason: FeatureRemovalReason::Expired,
            ..
        }]
    ));
}

#[test]
fn replacement_removes_disappeared_reports_and_upserts_the_new_set() {
    let set = recorded_set();
    let mut store = store();
    let first = store
        .accept(
            ingress(&set.records[..1], SourceProductRevision::new(1), 60_000),
            evaluation_time(100),
        )
        .expect("the first set must be valid")
        .into_publication()
        .expect("the first set must publish");
    let second = store
        .accept(
            ingress(&set.records[1..2], SourceProductRevision::new(2), 60_000),
            evaluation_time(200),
        )
        .expect("the replacement set must be valid")
        .into_publication()
        .expect("the replacement set must publish");
    let resolver = recorded_resolver(&set.records);

    let deltas = map_snapshot_transition(Some(first.envelope()), second.envelope(), &resolver);

    assert_eq!(deltas.len(), 2);
    assert!(matches!(
        &deltas[0],
        FeatureDelta::Remove {
            reason: FeatureRemovalReason::Replacement,
            ..
        }
    ));
    let FeatureDelta::Upsert(WeatherFeature::TextReport(feature)) = &deltas[1] else {
        panic!("the replacement report must upsert");
    };
    assert_eq!(
        feature
            .id()
            .station_id()
            .expect("a text feature must have a station")
            .as_str(),
        "MMMD"
    );
    assert_eq!(feature.revisions().snapshot_revision().get(), 2);
    assert_eq!(feature.revisions().product_revision().get(), 2);
}

#[test]
fn global_block_produces_no_feature_and_no_error() {
    let mut store = store();
    let product = weather_product(
        FieldState::absent(AbsentField::new(
            AbsenceReason::Unsupported,
            AbsenceEvidence::Source {
                time: time_evidence(),
                quality: QualityEvidence::Unavailable,
                provenance: provenance(),
            },
        )),
        60_000,
    );
    let current = store
        .accept(
            WeatherIngress::product(
                WeatherProductId::new("fisb-product-63"),
                SourceProductRevision::new(1),
                product,
            ),
            evaluation_time(100),
        )
        .expect("the Global Block evidence must be valid")
        .into_publication()
        .expect("the Global Block evidence must publish");

    let deltas = map_snapshot_transition(None, current.envelope(), &missing_position);

    assert!(deltas.is_empty());
}

#[test]
fn a_report_without_a_station_position_produces_no_feature() {
    let set = recorded_set();
    let current = publish_product(&set.records[..1], SourceProductRevision::new(1), 60_000);

    let deltas = map_snapshot_transition(None, current.envelope(), &missing_position);

    assert!(deltas.is_empty());
}

#[cfg(feature = "geojson")]
#[test]
fn geojson_uses_lon_lat_and_keeps_the_typed_report_evidence() {
    let set = recorded_set();
    let current = publish_product(&set.records[..1], SourceProductRevision::new(1), 60_000);
    let resolver = recorded_resolver(&set.records);
    let delta = map_snapshot_transition(None, current.envelope(), &resolver)
        .into_iter()
        .next()
        .expect("the recorded METAR must map");

    let encoded = crate::encode_feature_delta(&delta).expect("the feature must encode");
    let value: serde_json::Value =
        serde_json::from_str(&encoded).expect("the feature must be JSON");

    assert_eq!(value["type"], "Feature");
    assert_eq!(value["geometry"]["type"], "Point");
    assert_eq!(
        value["geometry"]["coordinates"],
        serde_json::json!([-77.8401, 40.8534])
    );
    assert_eq!(value["properties"]["operation"], "upsert");
    assert_eq!(value["properties"]["feature_kind"], "text_report");
    assert_eq!(value["properties"]["snapshot_revision"], 1);
    assert_eq!(value["properties"]["product_revision"], 1);
    assert_eq!(value["properties"]["flight_category"], "vfr");
    assert_eq!(
        value["properties"]["report"]["text"]["value"],
        set.records[0].report_text
    );
    assert_eq!(
        value["properties"]["report"]["text"]["provenance"]["origin"]["kind"],
        "replay"
    );
}

#[cfg(feature = "geojson")]
#[test]
fn geojson_removal_has_null_geometry_and_a_reason() {
    let set = recorded_set();
    let current = publish_product(&set.records[..1], SourceProductRevision::new(1), 60_000);
    let resolver = recorded_resolver(&set.records);
    let FeatureDelta::Upsert(feature) =
        map_snapshot_transition(None, current.envelope(), &resolver)
            .into_iter()
            .next()
            .expect("the recorded METAR must map")
    else {
        panic!("the first change must be an upsert");
    };
    let removal = FeatureDelta::Remove {
        id: feature.id().clone(),
        reason: FeatureRemovalReason::Replacement,
    };

    let encoded = crate::encode_feature_delta(&removal).expect("the removal must encode");
    let value: serde_json::Value =
        serde_json::from_str(&encoded).expect("the removal must be JSON");

    assert!(value["geometry"].is_null());
    assert_eq!(value["properties"]["operation"], "remove");
    assert_eq!(value["properties"]["reason"], "replacement");
}

fn recorded_set() -> RecordedMetarSet {
    serde_json::from_str(RECORDED_METARS_JSON).expect("the recorded METAR set must be valid")
}

fn recorded_resolver(
    records: &[RecordedMetar],
) -> impl Fn(&WeatherStationId) -> Option<Wgs84Position> + '_ {
    |station| {
        records
            .iter()
            .find(|record| record.station_id == station.as_str())
            .and_then(|record| Wgs84Position::new(record.latitude_deg, record.longitude_deg))
    }
}

fn missing_position(_: &WeatherStationId) -> Option<Wgs84Position> {
    None
}

fn publish_product(
    records: &[RecordedMetar],
    revision: SourceProductRevision,
    maximum_age_ms: u64,
) -> WeatherSnapshotHandle {
    store()
        .accept(
            ingress(records, revision, maximum_age_ms),
            evaluation_time(100),
        )
        .expect("the recorded METAR set must be valid")
        .into_publication()
        .expect("the recorded METAR set must publish")
}

fn store() -> WeatherStore {
    WeatherStore::new(StoreConfig::default(), ProducerInstanceId::new(17))
        .expect("the default store configuration must be valid")
}

fn ingress(
    records: &[RecordedMetar],
    revision: SourceProductRevision,
    maximum_age_ms: u64,
) -> WeatherIngress {
    let reports = records.iter().map(text_report).collect();
    WeatherIngress::product(
        WeatherProductId::new(PRODUCT_ID),
        revision,
        weather_product(present(WeatherValue::TextReports(reports)), maximum_age_ms),
    )
}

fn weather_product(typed_value: FieldState<WeatherValue>, maximum_age_ms: u64) -> WeatherProduct {
    WeatherProduct::new(
        field(WeatherPayload::new(
            "application/vnd.airmass.recorded-metars+json",
            RECORDED_METARS_JSON.as_bytes().to_vec(),
        )),
        typed_value,
        absent(AbsenceReason::NotObserved),
        absent(AbsenceReason::NotObserved),
        absent(AbsenceReason::NotObserved),
        present(DurationMillis::new(maximum_age_ms)),
    )
}

fn text_report(record: &RecordedMetar) -> TextWeatherReport {
    let report_time = WeatherReportTime::new(
        record.observation_day,
        record.observation_hour,
        record.observation_minute,
    )
    .expect("the recorded report time must be valid");
    assert!(record.raw_text.contains(&record.report_text));
    TextWeatherReport::new(
        present(WeatherReportType::Metar),
        present(WeatherStationId::new(record.station_id.clone())),
        present(report_time),
        field(record.report_text.clone()),
        present(record.flight_category),
    )
}

fn field<T>(value: T) -> TimedField<T> {
    TimedField::new(
        value,
        time_evidence(),
        QualityEvidence::Unavailable,
        provenance(),
    )
}

fn present<T>(value: T) -> FieldState<T> {
    FieldState::present(field(value))
}

fn absent<T>(reason: AbsenceReason) -> FieldState<T> {
    FieldState::absent(AbsentField::new(reason, AbsenceEvidence::Unknown))
}

fn provenance() -> ProductProvenance {
    ProductProvenance::new(
        SourceRef::new(
            SourceId::new("aviation-weather-center"),
            SourceEpoch::new(1),
        ),
        WeatherOrigin::Replay,
        DeliveryPath::new("recorded-fixture"),
        Some(RawProductIdentity::new(
            "aviation-weather-center/metar-cache",
            "recorded-metar-set",
        )),
    )
}

const fn time_evidence() -> TimeEvidence {
    TimeEvidence::new(MonotonicTime::from_micros(100), SourceTime::Unknown)
}

const fn evaluation_time(monotonic_micros: u64) -> EvaluationTime {
    EvaluationTime::new(
        UtcTime::from_unix_millis(0),
        MonotonicTime::from_micros(monotonic_micros),
    )
}
