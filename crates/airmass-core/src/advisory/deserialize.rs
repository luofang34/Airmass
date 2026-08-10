//! Preserves advisory invariants during deserialization.

use alloc::vec::Vec;

use serde::{Deserialize, Deserializer, de::Error as _};

use super::{
    AdvisoryAltitude, AdvisoryAltitudeBand, AdvisoryGeometry, AdvisoryLine, AdvisoryPolygon,
    AdvisoryPosition, AdvisoryRing, AdvisoryValidity, WeatherAdvisory, WeatherAdvisoryId,
    WeatherAdvisoryType,
};
use crate::{FieldState, UtcInterval, UtcTime};

#[derive(Deserialize)]
#[serde(rename_all = "snake_case", tag = "kind", content = "value")]
enum ValidityFields {
    Period(UtcInterval),
    Point(UtcTime),
}

impl<'de> Deserialize<'de> for AdvisoryValidity {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let fields = ValidityFields::deserialize(deserializer)?;
        let value = match fields {
            ValidityFields::Period(interval) => Self::Period(interval),
            ValidityFields::Point(instant) => Self::Point(instant),
        };
        value
            .lifecycle_interval()
            .map(|_| value)
            .map_err(D::Error::custom)
    }
}

#[derive(Deserialize)]
struct PositionFields {
    latitude_e6: i32,
    longitude_e6: i32,
}

impl<'de> Deserialize<'de> for AdvisoryPosition {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let fields = PositionFields::deserialize(deserializer)?;
        Self::new(fields.latitude_e6, fields.longitude_e6).map_err(D::Error::custom)
    }
}

#[derive(Deserialize)]
struct RingFields {
    positions: Vec<AdvisoryPosition>,
}

impl<'de> Deserialize<'de> for AdvisoryRing {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let fields = RingFields::deserialize(deserializer)?;
        Self::new(fields.positions).map_err(D::Error::custom)
    }
}

#[derive(Deserialize)]
struct LineFields {
    positions: Vec<AdvisoryPosition>,
}

impl<'de> Deserialize<'de> for AdvisoryLine {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let fields = LineFields::deserialize(deserializer)?;
        Self::new(fields.positions).map_err(D::Error::custom)
    }
}

#[derive(Deserialize)]
struct PolygonFields {
    rings: Vec<AdvisoryRing>,
}

impl<'de> Deserialize<'de> for AdvisoryPolygon {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let fields = PolygonFields::deserialize(deserializer)?;
        Self::new(fields.rings).map_err(D::Error::custom)
    }
}

#[derive(Deserialize)]
struct AltitudeBandFields {
    base: AdvisoryAltitude,
    top: AdvisoryAltitude,
}

impl<'de> Deserialize<'de> for AdvisoryAltitudeBand {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let fields = AltitudeBandFields::deserialize(deserializer)?;
        Self::new(fields.base, fields.top).map_err(D::Error::custom)
    }
}

#[derive(Deserialize)]
struct WeatherAdvisoryFields {
    identity: FieldState<WeatherAdvisoryId>,
    advisory_type: FieldState<WeatherAdvisoryType>,
    validity: FieldState<AdvisoryValidity>,
    geometry: FieldState<AdvisoryGeometry>,
    altitude: FieldState<AdvisoryAltitude>,
    altitude_band: FieldState<AdvisoryAltitudeBand>,
}

impl<'de> Deserialize<'de> for WeatherAdvisory {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let fields = WeatherAdvisoryFields::deserialize(deserializer)?;
        let value = Self::new(
            fields.identity,
            fields.advisory_type,
            fields.validity,
            fields.geometry,
            fields.altitude,
            fields.altitude_band,
        );
        value.validate_structure().map_err(D::Error::custom)?;
        Ok(value)
    }
}
