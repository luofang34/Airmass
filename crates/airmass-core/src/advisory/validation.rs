//! Validates advisory structure.

use thiserror::Error;

use super::{
    AdvisoryAltitudeBand, AdvisoryGeometry, AdvisoryGeometryError, AdvisoryPosition, AdvisoryRing,
    WeatherAdvisory,
};
use crate::UtcTime;

impl WeatherAdvisory {
    /// Validate source-derived values.
    ///
    /// # Errors
    ///
    /// Returns [`AdvisoryValidationError`] when validity, geometry, or vertical
    /// data is not valid.
    pub fn validate_structure(&self) -> Result<(), AdvisoryValidationError> {
        if let Some(field) = self.validity.as_present() {
            field.value.lifecycle_interval()?;
        }
        if let Some(field) = self.geometry.as_present() {
            validate_geometry(&field.value)?;
        }
        if let Some(field) = self.altitude_band.as_present() {
            validate_band(field.value)?;
        }
        if self.altitude.as_present().is_some() && self.altitude_band.as_present().is_some() {
            return Err(AdvisoryValidationError::ConflictingAltitudeFields);
        }
        Ok(())
    }
}

fn validate_geometry(geometry: &AdvisoryGeometry) -> Result<(), AdvisoryGeometryError> {
    match geometry {
        AdvisoryGeometry::Polygon(polygon) => {
            if polygon.rings.is_empty() {
                return Err(AdvisoryGeometryError::PolygonHasNoRing);
            }
            for ring in &polygon.rings {
                validate_ring(ring)?;
            }
        }
        AdvisoryGeometry::Polyline(line) => {
            if line.positions.len() < 2 {
                return Err(AdvisoryGeometryError::LineTooShort {
                    count: line.positions.len(),
                });
            }
            validate_positions(&line.positions)?;
        }
    }
    Ok(())
}

fn validate_ring(ring: &AdvisoryRing) -> Result<(), AdvisoryGeometryError> {
    if ring.positions.len() < 4 {
        return Err(AdvisoryGeometryError::RingTooShort {
            count: ring.positions.len(),
        });
    }
    if ring.positions.first() != ring.positions.last() {
        return Err(AdvisoryGeometryError::RingNotClosed);
    }
    validate_positions(&ring.positions)
}

fn validate_positions(positions: &[AdvisoryPosition]) -> Result<(), AdvisoryGeometryError> {
    for position in positions {
        AdvisoryPosition::new(position.latitude_e6, position.longitude_e6).map(|_| ())?;
    }
    Ok(())
}

fn validate_band(band: AdvisoryAltitudeBand) -> Result<(), AdvisoryGeometryError> {
    AdvisoryAltitudeBand::new(band.base, band.top).map(|_| ())
}

/// Failure to validate one advisory.
#[derive(Clone, Debug, Eq, Error, PartialEq)]
#[non_exhaustive]
pub enum AdvisoryValidationError {
    /// Geometry or altitude-band structure is invalid.
    #[error("advisory geometry or altitude band is invalid")]
    Geometry(#[from] AdvisoryGeometryError),
    /// A validity period has no positive duration.
    #[error("advisory validity starts at {start:?} and ends at {end:?}")]
    InvalidValidity {
        /// Validity start.
        start: UtcTime,
        /// Validity end.
        end: UtcTime,
    },
    /// A point validity cannot map to a lifecycle interval.
    #[error("advisory point validity {instant:?} has no later millisecond")]
    PointValidityOverflow {
        /// Rejected point validity.
        instant: UtcTime,
    },
    /// A single altitude and an altitude band are both present.
    #[error("advisory has both a single altitude and an altitude band")]
    ConflictingAltitudeFields,
}
