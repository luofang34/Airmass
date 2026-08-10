//! Decodes graphical advisory altitude fields.

use airmass_core::{
    AbsenceReason, AdvisoryAltitude, AdvisoryAltitudeBand, AdvisoryAltitudeReference, FieldState,
};

use super::{AdvisoryDecodeError, Vertex, unsupported};
use crate::text_report::SourceEvidence;

type VerticalFields = (
    FieldState<AdvisoryAltitude>,
    FieldState<AdvisoryAltitudeBand>,
);

pub(super) fn fields(
    vertices: &[Vertex],
    reference: AdvisoryAltitudeReference,
    geometry_option: u8,
    evidence: &SourceEvidence,
) -> Result<VerticalFields, AdvisoryDecodeError> {
    let Some(first) = vertices.first() else {
        return Err(AdvisoryDecodeError::InsufficientPositions {
            count: 0,
            required: 1,
        });
    };
    let (mut base, mut top) = (first.altitude_feet, first.altitude_feet);
    for vertex in &vertices[1..] {
        base = base.min(vertex.altitude_feet);
        top = top.max(vertex.altitude_feet);
    }
    if matches!(geometry_option, 11 | 12) && base != top {
        return Err(AdvisoryDecodeError::InconsistentGraphicalField {
            field: "polyline altitude",
        });
    }
    if base == top {
        return Ok((
            evidence.present(AdvisoryAltitude::new(base, reference)),
            evidence.absent(AbsenceReason::NotObserved),
        ));
    }
    let band = AdvisoryAltitudeBand::new(
        AdvisoryAltitude::new(base, reference),
        AdvisoryAltitude::new(top, reference),
    )?;
    Ok((
        evidence.absent(AbsenceReason::NotObserved),
        evidence.present(band),
    ))
}

pub(super) fn altitude_reference(
    option: u8,
) -> Result<AdvisoryAltitudeReference, AdvisoryDecodeError> {
    match option {
        3 | 11 => Ok(AdvisoryAltitudeReference::MeanSeaLevel),
        4 | 12 => Ok(AdvisoryAltitudeReference::AboveGroundLevel),
        _ => unsupported("overlay geometry option", option),
    }
}
