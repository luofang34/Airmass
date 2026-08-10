//! Optional RFC 7946 encoding.

use alloc::{format, string::String, vec::Vec};

use airmass_core::{FlightCategory, WeatherAdvisoryType, WeatherReportType};
use serde::Serialize;
use thiserror::Error;

use crate::{
    AdvisoryShapeFeature, AdvisoryShapeGeometry, FeatureDelta, FeatureRemovalReason,
    TextReportFeature, WeatherFeature, WeatherFeatureId, WeatherFeatureKey,
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
/// A text report has Point geometry. An advisory has Polygon or LineString
/// geometry.
/// A removal has null geometry.
pub fn encode_feature_delta(delta: &FeatureDelta) -> Result<String, GeoJsonError> {
    serde_json::to_string(&GeoJsonFeature::from_delta(delta))
        .map_err(|source| GeoJsonError::Serialize { source })
}

#[derive(Serialize)]
struct GeoJsonFeature<'a> {
    #[serde(rename = "type")]
    feature_type: &'static str,
    id: String,
    geometry: Option<Geometry>,
    properties: FeatureProperties<'a>,
}

impl<'a> GeoJsonFeature<'a> {
    fn from_delta(delta: &'a FeatureDelta) -> Self {
        match delta {
            FeatureDelta::Upsert(WeatherFeature::TextReport(feature)) => Self::text_upsert(feature),
            FeatureDelta::Upsert(WeatherFeature::AdvisoryShape(feature)) => {
                Self::advisory_upsert(feature)
            }
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

    fn text_upsert(feature: &'a TextReportFeature) -> Self {
        let position = feature.position();
        let revisions = feature.revisions();
        Self {
            feature_type: "Feature",
            id: feature_id(feature.id()),
            geometry: Some(Geometry::Point(PointGeometry {
                geometry_type: "Point",
                coordinates: [position.longitude_deg(), position.latitude_deg()],
            })),
            properties: FeatureProperties::Text(TextProperties {
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

    fn advisory_upsert(feature: &'a AdvisoryShapeFeature) -> Self {
        let revisions = feature.revisions();
        Self {
            feature_type: "Feature",
            id: feature_id(feature.id()),
            geometry: Some(advisory_geometry(feature)),
            properties: FeatureProperties::Advisory(AdvisoryProperties {
                operation: "upsert",
                feature_kind: "advisory_shape",
                producer_instance_id: feature.id().producer_instance_id(),
                product_id: feature.id().product_id(),
                snapshot_revision: revisions.snapshot_revision(),
                product_revision: revisions.product_revision(),
                source_product_revision: revisions.source_product_revision(),
                source_advisory_index: feature.source_advisory_index(),
                advisory_id: feature.id().advisory_id(),
                advisory_type: feature
                    .advisory()
                    .advisory_type()
                    .as_present()
                    .map(|field| field.value),
                altitude_band: feature.advisory().altitude_band(),
                altitude: feature.advisory().altitude(),
                selection: feature.selection(),
                advisory: feature.advisory(),
            }),
        }
    }
}

fn advisory_geometry(feature: &AdvisoryShapeFeature) -> Geometry {
    match feature.geometry() {
        AdvisoryShapeGeometry::Polygon(rings) => Geometry::Polygon(PolygonGeometry {
            geometry_type: "Polygon",
            coordinates: rings
                .iter()
                .map(|ring| coordinates(ring.positions()))
                .collect(),
        }),
        AdvisoryShapeGeometry::Polyline(line) => Geometry::LineString(LineStringGeometry {
            geometry_type: "LineString",
            coordinates: coordinates(line.positions()),
        }),
    }
}

fn coordinates(positions: &[crate::Wgs84Position]) -> Vec<[f64; 2]> {
    positions
        .iter()
        .map(|position| [position.longitude_deg(), position.latitude_deg()])
        .collect()
}

fn feature_id(id: &WeatherFeatureId) -> String {
    match id.key() {
        WeatherFeatureKey::TextReport {
            report_type,
            station_id,
            occurrence,
        } => format!(
            "airmass:{}:{}:{}:{}:{}:{}:{}:point",
            id.producer_instance_id().get(),
            id.product_id().as_str().len(),
            id.product_id().as_str(),
            report_type_name(*report_type),
            station_id.as_str().len(),
            station_id.as_str(),
            occurrence,
        ),
        WeatherFeatureKey::Advisory {
            advisory_id,
            occurrence,
        } => format!(
            "airmass:{}:{}:{}:advisory:{}:{}:{}:shape",
            id.producer_instance_id().get(),
            id.product_id().as_str().len(),
            id.product_id().as_str(),
            advisory_id.as_str().len(),
            advisory_id.as_str(),
            occurrence,
        ),
    }
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
#[serde(untagged)]
enum Geometry {
    Point(PointGeometry),
    Polygon(PolygonGeometry),
    LineString(LineStringGeometry),
}

#[derive(Serialize)]
struct PointGeometry {
    #[serde(rename = "type")]
    geometry_type: &'static str,
    coordinates: [f64; 2],
}

#[derive(Serialize)]
struct PolygonGeometry {
    #[serde(rename = "type")]
    geometry_type: &'static str,
    coordinates: Vec<Vec<[f64; 2]>>,
}

#[derive(Serialize)]
struct LineStringGeometry {
    #[serde(rename = "type")]
    geometry_type: &'static str,
    coordinates: Vec<[f64; 2]>,
}

#[derive(Serialize)]
#[serde(untagged)]
enum FeatureProperties<'a> {
    Text(TextProperties<'a>),
    Advisory(AdvisoryProperties<'a>),
    Remove(RemoveProperties),
}

#[derive(Serialize)]
struct TextProperties<'a> {
    operation: &'static str,
    feature_kind: &'static str,
    producer_instance_id: airmass_core::ProducerInstanceId,
    product_id: &'a airmass_core::WeatherProductId,
    snapshot_revision: airmass_core::SnapshotRevision,
    product_revision: airmass_core::ProductRevision,
    source_product_revision: airmass_core::SourceProductRevision,
    source_report_index: u32,
    report_type: Option<WeatherReportType>,
    station_id: Option<&'a airmass_core::WeatherStationId>,
    flight_category: Option<FlightCategory>,
    selection: &'a airmass_core::SelectionExplanation,
    report: &'a airmass_core::TextWeatherReport,
}

#[derive(Serialize)]
struct AdvisoryProperties<'a> {
    operation: &'static str,
    feature_kind: &'static str,
    producer_instance_id: airmass_core::ProducerInstanceId,
    product_id: &'a airmass_core::WeatherProductId,
    snapshot_revision: airmass_core::SnapshotRevision,
    product_revision: airmass_core::ProductRevision,
    source_product_revision: airmass_core::SourceProductRevision,
    source_advisory_index: u32,
    advisory_id: Option<&'a airmass_core::WeatherAdvisoryId>,
    advisory_type: Option<WeatherAdvisoryType>,
    altitude_band: &'a airmass_core::FieldState<airmass_core::AdvisoryAltitudeBand>,
    altitude: &'a airmass_core::FieldState<airmass_core::AdvisoryAltitude>,
    selection: &'a airmass_core::SelectionExplanation,
    advisory: &'a airmass_core::WeatherAdvisory,
}

#[derive(Serialize)]
struct RemoveProperties {
    operation: &'static str,
    reason: &'static str,
}
