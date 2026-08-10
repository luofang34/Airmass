//! Typed weather advisories and their geometry.

use alloc::{string::String, vec::Vec};

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::{FieldState, UtcInterval, UtcTime};

mod deserialize;
#[cfg(test)]
mod tests;
mod validation;

pub use validation::AdvisoryValidationError;

const MAX_LATITUDE_E6: i32 = 90_000_000;
const MAX_LONGITUDE_E6: i32 = 180_000_000;

/// Stable source identity for one weather advisory.
#[derive(Clone, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(transparent)]
pub struct WeatherAdvisoryId(String);

impl WeatherAdvisoryId {
    /// Create an advisory identity.
    #[must_use]
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    /// Get the identity text.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Type of one weather advisory.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum WeatherAdvisoryType {
    /// Significant meteorological information.
    Sigmet,
    /// Convective significant meteorological information.
    ConvectiveSigmet,
    /// Airmen's meteorological information.
    Airmet,
    /// Graphical airmen's meteorological information.
    GAirmet,
    /// Center weather advisory.
    CenterWeatherAdvisory,
    /// Aviation weather warning.
    AviationWeatherWarning,
}

/// Source validity for one advisory.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case", tag = "kind", content = "value")]
#[non_exhaustive]
pub enum AdvisoryValidity {
    /// The advisory applies during one half-open interval.
    Period(UtcInterval),
    /// The advisory applies at one forecast instant.
    Point(UtcTime),
}

impl AdvisoryValidity {
    /// Get the first valid instant.
    #[must_use]
    pub const fn start(self) -> UtcTime {
        match self {
            Self::Period(interval) => interval.start,
            Self::Point(instant) => instant,
        }
    }

    /// Get the lifecycle interval used by the store.
    ///
    /// A point uses one millisecond because [`UtcTime`] has millisecond
    /// resolution.
    ///
    /// # Errors
    ///
    /// Returns [`AdvisoryValidationError`] when a period has an invalid order
    /// or a point has no later representable millisecond.
    pub fn lifecycle_interval(self) -> Result<UtcInterval, AdvisoryValidationError> {
        match self {
            Self::Period(interval) if interval.has_valid_order() => Ok(interval),
            Self::Period(interval) => Err(AdvisoryValidationError::InvalidValidity {
                start: interval.start,
                end: interval.end,
            }),
            Self::Point(instant) => instant
                .as_unix_millis()
                .checked_add(1)
                .map(|end| UtcInterval::new(instant, UtcTime::from_unix_millis(end)))
                .ok_or(AdvisoryValidationError::PointValidityOverflow { instant }),
        }
    }
}

/// One fixed-point WGS84 position in microdegrees.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
pub struct AdvisoryPosition {
    latitude_e6: i32,
    longitude_e6: i32,
}

impl AdvisoryPosition {
    /// Create a position from signed microdegrees.
    ///
    /// # Errors
    ///
    /// Returns [`AdvisoryGeometryError`] when a coordinate is out of range.
    pub fn new(latitude_e6: i32, longitude_e6: i32) -> Result<Self, AdvisoryGeometryError> {
        if !(-MAX_LATITUDE_E6..=MAX_LATITUDE_E6).contains(&latitude_e6) {
            return Err(AdvisoryGeometryError::LatitudeOutOfRange { latitude_e6 });
        }
        if !(-MAX_LONGITUDE_E6..=MAX_LONGITUDE_E6).contains(&longitude_e6) {
            return Err(AdvisoryGeometryError::LongitudeOutOfRange { longitude_e6 });
        }
        Ok(Self {
            latitude_e6,
            longitude_e6,
        })
    }

    /// Get latitude in signed microdegrees.
    #[must_use]
    pub const fn latitude_e6(self) -> i32 {
        self.latitude_e6
    }

    /// Get longitude in signed microdegrees.
    #[must_use]
    pub const fn longitude_e6(self) -> i32 {
        self.longitude_e6
    }
}

/// One closed position ring.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct AdvisoryRing {
    positions: Vec<AdvisoryPosition>,
}

impl AdvisoryRing {
    /// Create a closed ring with at least four positions.
    ///
    /// # Errors
    ///
    /// Returns [`AdvisoryGeometryError`] when the ring is too short or open.
    pub fn new(positions: Vec<AdvisoryPosition>) -> Result<Self, AdvisoryGeometryError> {
        if positions.len() < 4 {
            return Err(AdvisoryGeometryError::RingTooShort {
                count: positions.len(),
            });
        }
        if positions.first() != positions.last() {
            return Err(AdvisoryGeometryError::RingNotClosed);
        }
        Ok(Self { positions })
    }

    /// Get the positions in source order.
    #[must_use]
    pub fn positions(&self) -> &[AdvisoryPosition] {
        &self.positions
    }
}

/// One line with two or more positions.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct AdvisoryLine {
    positions: Vec<AdvisoryPosition>,
}

impl AdvisoryLine {
    /// Create a line with at least two positions.
    ///
    /// # Errors
    ///
    /// Returns [`AdvisoryGeometryError`] when the line is too short.
    pub fn new(positions: Vec<AdvisoryPosition>) -> Result<Self, AdvisoryGeometryError> {
        if positions.len() < 2 {
            return Err(AdvisoryGeometryError::LineTooShort {
                count: positions.len(),
            });
        }
        Ok(Self { positions })
    }

    /// Get the positions in source order.
    #[must_use]
    pub fn positions(&self) -> &[AdvisoryPosition] {
        &self.positions
    }
}

/// One polygon with one or more closed rings.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct AdvisoryPolygon {
    rings: Vec<AdvisoryRing>,
}

impl AdvisoryPolygon {
    /// Create a polygon with one or more rings.
    ///
    /// # Errors
    ///
    /// Returns [`AdvisoryGeometryError`] when there is no ring.
    pub fn new(rings: Vec<AdvisoryRing>) -> Result<Self, AdvisoryGeometryError> {
        if rings.is_empty() {
            return Err(AdvisoryGeometryError::PolygonHasNoRing);
        }
        Ok(Self { rings })
    }

    /// Get the rings in source order.
    #[must_use]
    pub fn rings(&self) -> &[AdvisoryRing] {
        &self.rings
    }
}

/// Geometry for one weather advisory.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case", tag = "kind", content = "value")]
#[non_exhaustive]
pub enum AdvisoryGeometry {
    /// Polygon geometry.
    Polygon(AdvisoryPolygon),
    /// Line geometry.
    Polyline(AdvisoryLine),
}

/// Reference for one advisory altitude.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum AdvisoryAltitudeReference {
    /// Height above mean sea level.
    MeanSeaLevel,
    /// Height above ground level.
    AboveGroundLevel,
    /// Pressure flight level.
    FlightLevel,
    /// The surface.
    Surface,
}

/// One advisory altitude in feet.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct AdvisoryAltitude {
    feet: u32,
    reference: AdvisoryAltitudeReference,
}

impl AdvisoryAltitude {
    /// Create an altitude.
    #[must_use]
    pub const fn new(feet: u32, reference: AdvisoryAltitudeReference) -> Self {
        Self { feet, reference }
    }

    /// Get the altitude in feet.
    #[must_use]
    pub const fn feet(self) -> u32 {
        self.feet
    }

    /// Get the altitude reference.
    #[must_use]
    pub const fn reference(self) -> AdvisoryAltitudeReference {
        self.reference
    }
}

/// Lower and upper limits for one advisory.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
pub struct AdvisoryAltitudeBand {
    base: AdvisoryAltitude,
    top: AdvisoryAltitude,
}

impl AdvisoryAltitudeBand {
    /// Create a band whose base is not above its top.
    ///
    /// # Errors
    ///
    /// Returns [`AdvisoryGeometryError`] when the altitude order is invalid.
    pub fn new(
        base: AdvisoryAltitude,
        top: AdvisoryAltitude,
    ) -> Result<Self, AdvisoryGeometryError> {
        if base.feet() > top.feet() {
            return Err(AdvisoryGeometryError::AltitudeOrder {
                base_feet: base.feet(),
                top_feet: top.feet(),
            });
        }
        Ok(Self { base, top })
    }

    /// Get the lower limit.
    #[must_use]
    pub const fn base(self) -> AdvisoryAltitude {
        self.base
    }

    /// Get the upper limit.
    #[must_use]
    pub const fn top(self) -> AdvisoryAltitude {
        self.top
    }
}

/// One source-derived weather advisory.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct WeatherAdvisory {
    identity: FieldState<WeatherAdvisoryId>,
    advisory_type: FieldState<WeatherAdvisoryType>,
    validity: FieldState<AdvisoryValidity>,
    geometry: FieldState<AdvisoryGeometry>,
    altitude: FieldState<AdvisoryAltitude>,
    altitude_band: FieldState<AdvisoryAltitudeBand>,
}

impl WeatherAdvisory {
    /// Create an advisory from its source-derived fields.
    #[must_use]
    pub const fn new(
        identity: FieldState<WeatherAdvisoryId>,
        advisory_type: FieldState<WeatherAdvisoryType>,
        validity: FieldState<AdvisoryValidity>,
        geometry: FieldState<AdvisoryGeometry>,
        altitude: FieldState<AdvisoryAltitude>,
        altitude_band: FieldState<AdvisoryAltitudeBand>,
    ) -> Self {
        Self {
            identity,
            advisory_type,
            validity,
            geometry,
            altitude,
            altitude_band,
        }
    }

    /// Get the advisory identity field.
    #[must_use]
    pub const fn identity(&self) -> &FieldState<WeatherAdvisoryId> {
        &self.identity
    }

    /// Get the advisory type field.
    #[must_use]
    pub const fn advisory_type(&self) -> &FieldState<WeatherAdvisoryType> {
        &self.advisory_type
    }

    /// Get the advisory validity field.
    #[must_use]
    pub const fn validity(&self) -> &FieldState<AdvisoryValidity> {
        &self.validity
    }

    /// Get the advisory geometry field.
    #[must_use]
    pub const fn geometry(&self) -> &FieldState<AdvisoryGeometry> {
        &self.geometry
    }

    /// Get the optional single-altitude field.
    #[must_use]
    pub const fn altitude(&self) -> &FieldState<AdvisoryAltitude> {
        &self.altitude
    }

    /// Get the optional altitude-band field.
    #[must_use]
    pub const fn altitude_band(&self) -> &FieldState<AdvisoryAltitudeBand> {
        &self.altitude_band
    }
}

/// Failure to construct advisory geometry.
#[derive(Clone, Debug, Eq, Error, PartialEq)]
#[non_exhaustive]
pub enum AdvisoryGeometryError {
    /// Latitude is outside its WGS84 range.
    #[error("advisory latitude {latitude_e6} microdegrees is outside -90000000 through 90000000")]
    LatitudeOutOfRange {
        /// Rejected latitude.
        latitude_e6: i32,
    },
    /// Longitude is outside its WGS84 range.
    #[error(
        "advisory longitude {longitude_e6} microdegrees is outside -180000000 through 180000000"
    )]
    LongitudeOutOfRange {
        /// Rejected longitude.
        longitude_e6: i32,
    },
    /// A ring has fewer than four positions.
    #[error("advisory ring has {count} positions; at least 4 are required")]
    RingTooShort {
        /// Supplied position count.
        count: usize,
    },
    /// The first and last positions in a ring differ.
    #[error("advisory ring is not closed")]
    RingNotClosed,
    /// A line has fewer than two positions.
    #[error("advisory line has {count} positions; at least 2 are required")]
    LineTooShort {
        /// Supplied position count.
        count: usize,
    },
    /// A polygon has no rings.
    #[error("advisory polygon has no ring")]
    PolygonHasNoRing,
    /// The altitude base is above the top.
    #[error("advisory altitude base {base_feet} ft is above top {top_feet} ft")]
    AltitudeOrder {
        /// Rejected base altitude.
        base_feet: u32,
        /// Rejected top altitude.
        top_feet: u32,
    },
}
