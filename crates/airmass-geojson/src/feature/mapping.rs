//! Maps snapshot transitions to typed feature changes.

use alloc::vec::Vec;

use airmass_core::{
    AdvisoryGeometry, TextWeatherReport, WeatherAdvisory, WeatherAdvisoryId, WeatherProductId,
    WeatherProductSnapshot, WeatherProductState, WeatherReportType, WeatherSnapshotEnvelope,
    WeatherStationId, WeatherValue,
};

use super::{
    AdvisoryShapeFeature, AdvisoryShapeGeometry, AdvisoryShapeLine, AdvisoryShapeRing,
    FeatureDelta, FeatureRemovalReason, StationPositionResolver, TextReportFeature, WeatherFeature,
    WeatherFeatureId, WeatherFeatureKey, WeatherFeatureRevisions,
};
use crate::Wgs84Position;

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
    append_new_products(&mut deltas, previous, current, resolver);
    deltas
}

fn append_new_products<R>(
    deltas: &mut Vec<FeatureDelta>,
    previous: &WeatherSnapshotEnvelope,
    current: &WeatherSnapshotEnvelope,
    resolver: &R,
) where
    R: StationPositionResolver + ?Sized,
{
    for next in current.snapshot().products() {
        if previous.snapshot().product(next.product_id()).is_none() {
            append_upserts(deltas, current, next, resolver);
        }
    }
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
    let reason = removal_reason(next.state());
    for feature in map_product_features(previous, prior, resolver) {
        if !next_features.iter().any(|next| next.id() == feature.id()) {
            deltas.push(FeatureDelta::Remove {
                id: feature.id().clone(),
                reason,
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
    let Some(value) = product
        .state()
        .available()
        .and_then(|available| available.typed_value().as_present())
        .map(|field| &field.value)
    else {
        return Vec::new();
    };
    match value {
        WeatherValue::TextReports(reports) => reports
            .iter()
            .enumerate()
            .filter_map(|(index, report)| {
                map_report(envelope, product, reports, index, report, resolver)
            })
            .map(WeatherFeature::TextReport)
            .collect(),
        WeatherValue::Advisories(advisories) => advisories
            .iter()
            .enumerate()
            .filter_map(|(index, advisory)| {
                map_advisory(envelope, product, advisories, index, advisory)
            })
            .map(WeatherFeature::AdvisoryShape)
            .collect(),
        _ => Vec::new(),
    }
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
    let position = resolver.station_position(id.station_id()?)?;
    Some(TextReportFeature {
        id,
        position,
        source_report_index: u32::try_from(index).ok()?,
        revisions: revisions(envelope, product),
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
        key: WeatherFeatureKey::TextReport {
            report_type,
            station_id: station_id.clone(),
            occurrence: u32::try_from(occurrence).ok()?,
        },
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

fn map_advisory(
    envelope: &WeatherSnapshotEnvelope,
    product: &WeatherProductSnapshot,
    advisories: &[WeatherAdvisory],
    index: usize,
    advisory: &WeatherAdvisory,
) -> Option<AdvisoryShapeFeature> {
    let identity = &advisory.identity().as_present()?.value;
    let geometry = map_advisory_geometry(&advisory.geometry().as_present()?.value)?;
    let occurrence = advisories
        .get(..index)?
        .iter()
        .filter(|prior| same_advisory_key(prior, identity))
        .count();
    Some(AdvisoryShapeFeature {
        id: WeatherFeatureId {
            producer_instance_id: envelope.producer_instance_id(),
            product_id: product.product_id().clone(),
            key: WeatherFeatureKey::Advisory {
                advisory_id: identity.clone(),
                occurrence: u32::try_from(occurrence).ok()?,
            },
        },
        geometry,
        source_advisory_index: u32::try_from(index).ok()?,
        revisions: revisions(envelope, product),
        selection: product.selection().clone(),
        advisory: advisory.clone(),
    })
}

fn map_advisory_geometry(geometry: &AdvisoryGeometry) -> Option<AdvisoryShapeGeometry> {
    match geometry {
        AdvisoryGeometry::Polygon(polygon) => {
            let mut rings = Vec::new();
            for ring in polygon.rings() {
                rings.push(AdvisoryShapeRing {
                    positions: map_positions(ring.positions())?,
                });
            }
            Some(AdvisoryShapeGeometry::Polygon(rings))
        }
        AdvisoryGeometry::Polyline(line) => {
            Some(AdvisoryShapeGeometry::Polyline(AdvisoryShapeLine {
                positions: map_positions(line.positions())?,
            }))
        }
        _ => None,
    }
}

fn map_positions(positions: &[airmass_core::AdvisoryPosition]) -> Option<Vec<Wgs84Position>> {
    positions
        .iter()
        .map(|position| {
            Wgs84Position::new(
                f64::from(position.latitude_e6()) / 1_000_000.0,
                f64::from(position.longitude_e6()) / 1_000_000.0,
            )
        })
        .collect()
}

fn same_advisory_key(advisory: &WeatherAdvisory, identity: &WeatherAdvisoryId) -> bool {
    advisory
        .identity()
        .as_present()
        .is_some_and(|prior| prior.value == *identity)
}

fn revisions(
    envelope: &WeatherSnapshotEnvelope,
    product: &WeatherProductSnapshot,
) -> WeatherFeatureRevisions {
    WeatherFeatureRevisions {
        snapshot_revision: envelope.snapshot_revision(),
        product_revision: product.product_revision(),
        source_product_revision: product.source_product_revision(),
    }
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
