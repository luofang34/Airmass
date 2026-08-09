//! Typed weather feature changes.

use alloc::vec::Vec;

use airmass_core::{
    ProducerInstanceId, ProductRevision, SelectionExplanation, SnapshotRevision,
    SourceProductRevision, TextWeatherReport, WeatherProductId, WeatherProductSnapshot,
    WeatherProductState, WeatherReportType, WeatherSnapshotEnvelope, WeatherStationId,
};

use crate::Wgs84Position;

#[cfg(test)]
mod tests;

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
    report_type: WeatherReportType,
    station_id: WeatherStationId,
    occurrence: u32,
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

    /// Get the report type.
    #[must_use]
    pub const fn report_type(&self) -> WeatherReportType {
        self.report_type
    }

    /// Get the station identity.
    #[must_use]
    pub const fn station_id(&self) -> &WeatherStationId {
        &self.station_id
    }

    /// Get the zero-based occurrence for this report type and station.
    #[must_use]
    pub const fn occurrence(&self) -> u32 {
        self.occurrence
    }
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

/// One renderer-neutral weather feature.
#[non_exhaustive]
#[derive(Clone, Debug, PartialEq)]
pub enum WeatherFeature {
    /// Point feature for a typed text report.
    TextReport(TextReportFeature),
}

impl WeatherFeature {
    /// Get the stable feature identity.
    #[must_use]
    pub const fn id(&self) -> &WeatherFeatureId {
        match self {
            Self::TextReport(feature) => feature.id(),
        }
    }

    /// Get a text report feature when this feature contains one.
    #[must_use]
    pub const fn text_report(&self) -> Option<&TextReportFeature> {
        match self {
            Self::TextReport(feature) => Some(feature),
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

/// Map a weather snapshot transition into renderer-neutral feature changes.
///
/// Pass `None` for the first snapshot. The resolver must give a stable answer
/// while the caller maps one sequence of snapshots. A report with no type,
/// station, or station position produces no feature and no error.
#[must_use]
pub fn map_snapshot_transition<R>(
    previous: Option<&WeatherSnapshotEnvelope>,
    current: &WeatherSnapshotEnvelope,
    resolver: &R,
) -> Vec<FeatureDelta>
where
    R: StationPositionResolver + ?Sized,
{
    let Some(previous) = previous else {
        return map_initial_snapshot(current, resolver);
    };
    if previous.producer_instance_id() != current.producer_instance_id() {
        return map_producer_replacement(previous, current, resolver);
    }

    let mut deltas = Vec::new();
    for prior in previous.snapshot().products() {
        let Some(next) = current.snapshot().product(prior.product_id()) else {
            append_removals(
                &mut deltas,
                previous,
                prior,
                resolver,
                FeatureRemovalReason::Replacement,
            );
            continue;
        };
        if prior.product_revision() != next.product_revision() {
            append_product_transition(&mut deltas, previous, prior, current, next, resolver);
        }
    }
    for next in current.snapshot().products() {
        if previous.snapshot().product(next.product_id()).is_none() {
            append_upserts(&mut deltas, current, next, resolver);
        }
    }
    deltas
}

fn map_initial_snapshot<R>(current: &WeatherSnapshotEnvelope, resolver: &R) -> Vec<FeatureDelta>
where
    R: StationPositionResolver + ?Sized,
{
    let mut deltas = Vec::new();
    for product in current.snapshot().products() {
        append_upserts(&mut deltas, current, product, resolver);
    }
    deltas
}

fn map_producer_replacement<R>(
    previous: &WeatherSnapshotEnvelope,
    current: &WeatherSnapshotEnvelope,
    resolver: &R,
) -> Vec<FeatureDelta>
where
    R: StationPositionResolver + ?Sized,
{
    let mut deltas = Vec::new();
    for product in previous.snapshot().products() {
        append_removals(
            &mut deltas,
            previous,
            product,
            resolver,
            FeatureRemovalReason::Replacement,
        );
    }
    for product in current.snapshot().products() {
        append_upserts(&mut deltas, current, product, resolver);
    }
    deltas
}

fn append_product_transition<R>(
    deltas: &mut Vec<FeatureDelta>,
    previous: &WeatherSnapshotEnvelope,
    prior: &WeatherProductSnapshot,
    current: &WeatherSnapshotEnvelope,
    next: &WeatherProductSnapshot,
    resolver: &R,
) where
    R: StationPositionResolver + ?Sized,
{
    let next_features = map_product_features(current, next, resolver);
    let removal_reason = removal_reason(next.state());
    for feature in map_product_features(previous, prior, resolver) {
        if !next_features.iter().any(|next| next.id() == feature.id()) {
            deltas.push(FeatureDelta::Remove {
                id: feature.id().clone(),
                reason: removal_reason,
            });
        }
    }
    deltas.extend(next_features.into_iter().map(FeatureDelta::Upsert));
}

fn append_removals<R>(
    deltas: &mut Vec<FeatureDelta>,
    envelope: &WeatherSnapshotEnvelope,
    product: &WeatherProductSnapshot,
    resolver: &R,
    reason: FeatureRemovalReason,
) where
    R: StationPositionResolver + ?Sized,
{
    deltas.extend(
        map_product_features(envelope, product, resolver)
            .into_iter()
            .map(|feature| FeatureDelta::Remove {
                id: feature.id().clone(),
                reason,
            }),
    );
}

fn append_upserts<R>(
    deltas: &mut Vec<FeatureDelta>,
    envelope: &WeatherSnapshotEnvelope,
    product: &WeatherProductSnapshot,
    resolver: &R,
) where
    R: StationPositionResolver + ?Sized,
{
    deltas.extend(
        map_product_features(envelope, product, resolver)
            .into_iter()
            .map(FeatureDelta::Upsert),
    );
}

fn map_product_features<R>(
    envelope: &WeatherSnapshotEnvelope,
    product: &WeatherProductSnapshot,
    resolver: &R,
) -> Vec<WeatherFeature>
where
    R: StationPositionResolver + ?Sized,
{
    let Some(reports) = text_reports(product) else {
        return Vec::new();
    };
    reports
        .iter()
        .enumerate()
        .filter_map(|(index, report)| {
            map_report(envelope, product, reports, index, report, resolver)
        })
        .map(WeatherFeature::TextReport)
        .collect()
}

fn text_reports(product: &WeatherProductSnapshot) -> Option<&[TextWeatherReport]> {
    product
        .state()
        .available()?
        .typed_value()
        .as_present()?
        .value
        .text_reports()
}

fn map_report<R>(
    envelope: &WeatherSnapshotEnvelope,
    product: &WeatherProductSnapshot,
    reports: &[TextWeatherReport],
    index: usize,
    report: &TextWeatherReport,
    resolver: &R,
) -> Option<TextReportFeature>
where
    R: StationPositionResolver + ?Sized,
{
    let id = report_id(envelope, product.product_id(), reports, index, report)?;
    let position = resolver.station_position(id.station_id())?;
    let source_report_index = u32::try_from(index).ok()?;
    Some(TextReportFeature {
        id,
        position,
        source_report_index,
        revisions: WeatherFeatureRevisions {
            snapshot_revision: envelope.snapshot_revision(),
            product_revision: product.product_revision(),
            source_product_revision: product.source_product_revision(),
        },
        selection: product.selection().clone(),
        report: report.clone(),
    })
}

fn report_id(
    envelope: &WeatherSnapshotEnvelope,
    product_id: &WeatherProductId,
    reports: &[TextWeatherReport],
    index: usize,
    report: &TextWeatherReport,
) -> Option<WeatherFeatureId> {
    let report_type = report.report_type().as_present()?.value;
    let station_id = &report.station_id().as_present()?.value;
    let occurrence = reports
        .get(..index)?
        .iter()
        .filter(|prior| same_report_key(prior, report_type, station_id))
        .count();
    Some(WeatherFeatureId {
        producer_instance_id: envelope.producer_instance_id(),
        product_id: product_id.clone(),
        report_type,
        station_id: station_id.clone(),
        occurrence: u32::try_from(occurrence).ok()?,
    })
}

fn same_report_key(
    report: &TextWeatherReport,
    report_type: WeatherReportType,
    station_id: &WeatherStationId,
) -> bool {
    let Some(prior_type) = report.report_type().as_present() else {
        return false;
    };
    let Some(prior_station) = report.station_id().as_present() else {
        return false;
    };
    prior_type.value == report_type && prior_station.value == *station_id
}

fn removal_reason(state: &WeatherProductState) -> FeatureRemovalReason {
    match state {
        WeatherProductState::Absent { absence }
            if absence.reason() == airmass_core::AbsenceReason::Expired =>
        {
            FeatureRemovalReason::Expired
        }
        WeatherProductState::Available { .. }
        | WeatherProductState::Cancelled { .. }
        | WeatherProductState::Absent { .. } => FeatureRemovalReason::Replacement,
    }
}
