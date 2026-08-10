//! Typed weather feature changes.

use alloc::vec::Vec;

use airmass_core::{
    ProducerInstanceId, ProductRevision, SelectionExplanation, SnapshotRevision,
    SourceProductRevision, TextWeatherReport, WeatherAdvisory, WeatherAdvisoryId, WeatherProductId,
    WeatherReportType, WeatherStationId,
};

use crate::Wgs84Position;

mod mapping;
#[cfg(test)]
mod tests;

pub use mapping::map_snapshot_transition;

/// Supplies a WGS84 position for one weather station.
///
/// The composition application can use its navigation-data source to implement
/// this trait. Airmass does not own station coordinates.
pub trait StationPositionResolver {
    /// Get the station position when it is known.
    fn station_position(&self, station_id: &WeatherStationId) -> Option<Wgs84Position>;
}

impl<F> StationPositionResolver for F
where
    F: Fn(&WeatherStationId) -> Option<Wgs84Position>,
{
    fn station_position(&self, station_id: &WeatherStationId) -> Option<Wgs84Position> {
        self(station_id)
    }
}

/// Stable identity of one renderer-neutral weather feature.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WeatherFeatureId {
    producer_instance_id: ProducerInstanceId,
    product_id: WeatherProductId,
    key: WeatherFeatureKey,
}

impl WeatherFeatureId {
    /// Get the producer identity that is part of this feature identity.
    #[must_use]
    pub const fn producer_instance_id(&self) -> ProducerInstanceId {
        self.producer_instance_id
    }

    /// Get the source-neutral product identity.
    #[must_use]
    pub const fn product_id(&self) -> &WeatherProductId {
        &self.product_id
    }

    /// Get the domain feature key.
    #[must_use]
    pub const fn key(&self) -> &WeatherFeatureKey {
        &self.key
    }

    /// Get the report type for a text report feature.
    #[must_use]
    pub const fn report_type(&self) -> Option<WeatherReportType> {
        match &self.key {
            WeatherFeatureKey::TextReport { report_type, .. } => Some(*report_type),
            WeatherFeatureKey::Advisory { .. } => None,
        }
    }

    /// Get the station identity for a text report feature.
    #[must_use]
    pub const fn station_id(&self) -> Option<&WeatherStationId> {
        match &self.key {
            WeatherFeatureKey::TextReport { station_id, .. } => Some(station_id),
            WeatherFeatureKey::Advisory { .. } => None,
        }
    }

    /// Get the advisory identity for an advisory shape feature.
    #[must_use]
    pub const fn advisory_id(&self) -> Option<&WeatherAdvisoryId> {
        match &self.key {
            WeatherFeatureKey::Advisory { advisory_id, .. } => Some(advisory_id),
            WeatherFeatureKey::TextReport { .. } => None,
        }
    }

    /// Get the zero-based occurrence for this feature key.
    #[must_use]
    pub const fn occurrence(&self) -> u32 {
        match &self.key {
            WeatherFeatureKey::TextReport { occurrence, .. }
            | WeatherFeatureKey::Advisory { occurrence, .. } => *occurrence,
        }
    }
}

/// Domain key for one weather feature.
#[derive(Clone, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum WeatherFeatureKey {
    /// Key for a text report point.
    TextReport {
        /// Report type.
        report_type: WeatherReportType,
        /// Weather station identity.
        station_id: WeatherStationId,
        /// Zero-based occurrence for this type and station.
        occurrence: u32,
    },
    /// Key for an advisory shape.
    Advisory {
        /// Source advisory identity.
        advisory_id: WeatherAdvisoryId,
        /// Zero-based occurrence for this advisory identity.
        occurrence: u32,
    },
}

/// Revisions that identify the source weather publication.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct WeatherFeatureRevisions {
    snapshot_revision: SnapshotRevision,
    product_revision: ProductRevision,
    source_product_revision: SourceProductRevision,
}

impl WeatherFeatureRevisions {
    /// Get the complete snapshot revision.
    #[must_use]
    pub const fn snapshot_revision(self) -> SnapshotRevision {
        self.snapshot_revision
    }

    /// Get the Airmass product revision.
    #[must_use]
    pub const fn product_revision(self) -> ProductRevision {
        self.product_revision
    }

    /// Get the selected source product revision.
    #[must_use]
    pub const fn source_product_revision(self) -> SourceProductRevision {
        self.source_product_revision
    }
}

/// One point feature for an aviation weather text report.
#[derive(Clone, Debug, PartialEq)]
pub struct TextReportFeature {
    id: WeatherFeatureId,
    position: Wgs84Position,
    source_report_index: u32,
    revisions: WeatherFeatureRevisions,
    selection: SelectionExplanation,
    report: TextWeatherReport,
}

impl TextReportFeature {
    /// Get the stable feature identity.
    #[must_use]
    pub const fn id(&self) -> &WeatherFeatureId {
        &self.id
    }

    /// Get the WGS84 position.
    #[must_use]
    pub const fn position(&self) -> Wgs84Position {
        self.position
    }

    /// Get the zero-based report position in the source product.
    #[must_use]
    pub const fn source_report_index(&self) -> u32 {
        self.source_report_index
    }

    /// Get the publication revisions.
    #[must_use]
    pub const fn revisions(&self) -> WeatherFeatureRevisions {
        self.revisions
    }

    /// Get the source selection explanation.
    #[must_use]
    pub const fn selection(&self) -> &SelectionExplanation {
        &self.selection
    }

    /// Get the complete typed report and its field evidence.
    #[must_use]
    pub const fn report(&self) -> &TextWeatherReport {
        &self.report
    }
}

/// One closed ring in an advisory shape.
#[derive(Clone, Debug, PartialEq)]
pub struct AdvisoryShapeRing {
    positions: Vec<Wgs84Position>,
}

impl AdvisoryShapeRing {
    /// Get the closed positions in source order.
    #[must_use]
    pub fn positions(&self) -> &[Wgs84Position] {
        &self.positions
    }
}

/// One line in an advisory shape.
#[derive(Clone, Debug, PartialEq)]
pub struct AdvisoryShapeLine {
    positions: Vec<Wgs84Position>,
}

impl AdvisoryShapeLine {
    /// Get the positions in source order.
    #[must_use]
    pub fn positions(&self) -> &[Wgs84Position] {
        &self.positions
    }
}

/// Geometry for one renderer-neutral advisory shape.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub enum AdvisoryShapeGeometry {
    /// Polygon rings.
    Polygon(Vec<AdvisoryShapeRing>),
    /// One polyline.
    Polyline(AdvisoryShapeLine),
}

/// One renderer-neutral shape feature for a weather advisory.
#[derive(Clone, Debug, PartialEq)]
pub struct AdvisoryShapeFeature {
    id: WeatherFeatureId,
    geometry: AdvisoryShapeGeometry,
    source_advisory_index: u32,
    revisions: WeatherFeatureRevisions,
    selection: SelectionExplanation,
    advisory: WeatherAdvisory,
}

impl AdvisoryShapeFeature {
    /// Get the stable feature identity.
    #[must_use]
    pub const fn id(&self) -> &WeatherFeatureId {
        &self.id
    }

    /// Get the shape geometry.
    #[must_use]
    pub const fn geometry(&self) -> &AdvisoryShapeGeometry {
        &self.geometry
    }

    /// Get polygon rings when this shape is a polygon.
    #[must_use]
    pub fn rings(&self) -> &[AdvisoryShapeRing] {
        match &self.geometry {
            AdvisoryShapeGeometry::Polygon(rings) => rings,
            AdvisoryShapeGeometry::Polyline(_) => &[],
        }
    }

    /// Get the line when this shape is a polyline.
    #[must_use]
    pub const fn line(&self) -> Option<&AdvisoryShapeLine> {
        match &self.geometry {
            AdvisoryShapeGeometry::Polyline(line) => Some(line),
            AdvisoryShapeGeometry::Polygon(_) => None,
        }
    }

    /// Get the zero-based advisory position in the source product.
    #[must_use]
    pub const fn source_advisory_index(&self) -> u32 {
        self.source_advisory_index
    }

    /// Get the publication revisions.
    #[must_use]
    pub const fn revisions(&self) -> WeatherFeatureRevisions {
        self.revisions
    }

    /// Get the source selection explanation.
    #[must_use]
    pub const fn selection(&self) -> &SelectionExplanation {
        &self.selection
    }

    /// Get the complete typed advisory and its field evidence.
    #[must_use]
    pub const fn advisory(&self) -> &WeatherAdvisory {
        &self.advisory
    }
}

/// One renderer-neutral weather feature.
#[non_exhaustive]
#[derive(Clone, Debug, PartialEq)]
#[allow(
    clippy::large_enum_variant,
    reason = "boxing would add an allocation to each typed feature"
)]
pub enum WeatherFeature {
    /// Point feature for a typed text report.
    TextReport(TextReportFeature),
    /// Shape feature for a typed weather advisory.
    AdvisoryShape(AdvisoryShapeFeature),
}

impl WeatherFeature {
    /// Get the stable feature identity.
    #[must_use]
    pub const fn id(&self) -> &WeatherFeatureId {
        match self {
            Self::TextReport(feature) => feature.id(),
            Self::AdvisoryShape(feature) => feature.id(),
        }
    }

    /// Get a text report feature when this feature contains one.
    #[must_use]
    pub const fn text_report(&self) -> Option<&TextReportFeature> {
        match self {
            Self::TextReport(feature) => Some(feature),
            Self::AdvisoryShape(_) => None,
        }
    }

    /// Get an advisory shape feature when this feature contains one.
    #[must_use]
    pub const fn advisory_shape(&self) -> Option<&AdvisoryShapeFeature> {
        match self {
            Self::AdvisoryShape(feature) => Some(feature),
            Self::TextReport(_) => None,
        }
    }
}

/// Reason that the adapter removes one feature.
#[non_exhaustive]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FeatureRemovalReason {
    /// The weather product expired.
    Expired,
    /// A new product state or producer replaced the feature.
    Replacement,
}

/// One renderer-neutral change to the weather feature set.
#[non_exhaustive]
#[derive(Clone, Debug, PartialEq)]
#[allow(
    clippy::large_enum_variant,
    reason = "boxing would add an allocation to each typed upsert"
)]
pub enum FeatureDelta {
    /// Place or replace one weather feature.
    Upsert(WeatherFeature),
    /// Remove one weather feature.
    Remove {
        /// Stable identity of the feature.
        id: WeatherFeatureId,
        /// Lifecycle cause of the removal.
        reason: FeatureRemovalReason,
    },
}
