//! Optional RFC 7946 encoding.

use alloc::{format, string::String};

use airmass_core::{FlightCategory, WeatherReportType};
use serde::Serialize;
use thiserror::Error;

use crate::{
    FeatureDelta, FeatureRemovalReason, TextReportFeature, WeatherFeature, WeatherFeatureId,
};

/// Failure to encode one typed feature change as GeoJSON.
#[non_exhaustive]
#[derive(Debug, Error)]
pub enum GeoJsonError {
    /// JSON serialization failed.
    #[error("could not serialize the weather feature as GeoJSON")]
    Serialize {
        /// JSON serialization error.
        #[source]
        source: serde_json::Error,
    },
}

/// Encode one typed feature change as an RFC 7946 Feature object.
///
/// An upsert contains a Point in longitude-latitude order. A removal contains
/// a null geometry.
pub fn encode_feature_delta(delta: &FeatureDelta) -> Result<String, GeoJsonError> {
    let feature = GeoJsonFeature::from_delta(delta);
    serde_json::to_string(&feature).map_err(|source| GeoJsonError::Serialize { source })
}

#[derive(Serialize)]
struct GeoJsonFeature<'a> {
    #[serde(rename = "type")]
    feature_type: &'static str,
    id: String,
    geometry: Option<PointGeometry>,
    properties: FeatureProperties<'a>,
}

impl<'a> GeoJsonFeature<'a> {
    fn from_delta(delta: &'a FeatureDelta) -> Self {
        match delta {
            FeatureDelta::Upsert(WeatherFeature::TextReport(feature)) => Self::upsert(feature),
            FeatureDelta::Remove { id, reason } => Self {
                feature_type: "Feature",
                id: feature_id(id),
                geometry: None,
                properties: FeatureProperties::Remove(RemoveProperties {
                    operation: "remove",
                    reason: removal_reason(*reason),
                }),
            },
        }
    }

    fn upsert(feature: &'a TextReportFeature) -> Self {
        let position = feature.position();
        let revisions = feature.revisions();
        Self {
            feature_type: "Feature",
            id: feature_id(feature.id()),
            geometry: Some(PointGeometry {
                geometry_type: "Point",
                coordinates: [position.longitude_deg(), position.latitude_deg()],
            }),
            properties: FeatureProperties::Upsert(UpsertProperties {
                operation: "upsert",
                feature_kind: "text_report",
                producer_instance_id: feature.id().producer_instance_id(),
                product_id: feature.id().product_id(),
                snapshot_revision: revisions.snapshot_revision(),
                product_revision: revisions.product_revision(),
                source_product_revision: revisions.source_product_revision(),
                source_report_index: feature.source_report_index(),
                report_type: feature.id().report_type(),
                station_id: feature.id().station_id(),
                flight_category: feature
                    .report()
                    .flight_category()
                    .as_present()
                    .map(|field| field.value),
                selection: feature.selection(),
                report: feature.report(),
            }),
        }
    }
}

fn feature_id(id: &WeatherFeatureId) -> String {
    let report_type = report_type_name(id.report_type());
    format!(
        "airmass:{}:{}:{}:{}:{}:{}:{}:{}",
        id.producer_instance_id().get(),
        id.product_id().as_str().len(),
        id.product_id().as_str(),
        report_type,
        id.station_id().as_str().len(),
        id.station_id().as_str(),
        id.occurrence(),
        "point"
    )
}

const fn report_type_name(report_type: WeatherReportType) -> &'static str {
    match report_type {
        WeatherReportType::Metar => "metar",
        WeatherReportType::Speci => "speci",
        WeatherReportType::Taf => "taf",
        WeatherReportType::AmendedTaf => "amended_taf",
        WeatherReportType::Pirep => "pirep",
        WeatherReportType::WindsAndTemperaturesAloft => "winds_and_temperatures_aloft",
        _ => "text_report",
    }
}

const fn removal_reason(reason: FeatureRemovalReason) -> &'static str {
    match reason {
        FeatureRemovalReason::Expired => "expired",
        FeatureRemovalReason::Replacement => "replacement",
    }
}

#[derive(Serialize)]
struct PointGeometry {
    #[serde(rename = "type")]
    geometry_type: &'static str,
    coordinates: [f64; 2],
}

#[derive(Serialize)]
#[serde(untagged)]
enum FeatureProperties<'a> {
    Upsert(UpsertProperties<'a>),
    Remove(RemoveProperties),
}

#[derive(Serialize)]
struct UpsertProperties<'a> {
    operation: &'static str,
    feature_kind: &'static str,
    producer_instance_id: airmass_core::ProducerInstanceId,
    product_id: &'a airmass_core::WeatherProductId,
    snapshot_revision: airmass_core::SnapshotRevision,
    product_revision: airmass_core::ProductRevision,
    source_product_revision: airmass_core::SourceProductRevision,
    source_report_index: u32,
    report_type: WeatherReportType,
    station_id: &'a airmass_core::WeatherStationId,
    flight_category: Option<FlightCategory>,
    selection: &'a airmass_core::SelectionExplanation,
    report: &'a airmass_core::TextWeatherReport,
}

#[derive(Serialize)]
struct RemoveProperties {
    operation: &'static str,
    reason: &'static str,
}
