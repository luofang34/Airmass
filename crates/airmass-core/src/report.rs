//! Typed aviation weather text reports.

use alloc::string::String;

use serde::{Deserialize, Serialize};

use crate::{FieldState, TimedField};

#[cfg(test)]
mod tests;

/// Type of an aviation weather text report.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum WeatherReportType {
    /// Routine aerodrome observation.
    Metar,
    /// Special aerodrome observation.
    Speci,
    /// Aerodrome forecast.
    Taf,
    /// Amended aerodrome forecast.
    AmendedTaf,
    /// Pilot weather report.
    Pirep,
    /// Winds and temperatures aloft forecast.
    WindsAndTemperaturesAloft,
}

/// Source station identity for a text report.
#[derive(Clone, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(transparent)]
pub struct WeatherStationId(String);

impl WeatherStationId {
    /// Create a weather station identity.
    #[must_use]
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    /// Get the source station identity.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// UTC day and time supplied in a text report.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct WeatherReportTime {
    day_of_month: u8,
    hour: u8,
    minute: u8,
}

impl WeatherReportTime {
    /// Create a report time when all fields are in range.
    #[must_use]
    pub const fn new(day_of_month: u8, hour: u8, minute: u8) -> Option<Self> {
        if day_of_month == 0 || day_of_month > 31 || hour > 23 || minute > 59 {
            None
        } else {
            Some(Self {
                day_of_month,
                hour,
                minute,
            })
        }
    }

    /// Get the UTC day of month.
    #[must_use]
    pub const fn day_of_month(self) -> u8 {
        self.day_of_month
    }

    /// Get the UTC hour.
    #[must_use]
    pub const fn hour(self) -> u8 {
        self.hour
    }

    /// Get the UTC minute.
    #[must_use]
    pub const fn minute(self) -> u8 {
        self.minute
    }
}

/// Visibility expressed as an exact number of statute miles.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct StatuteMiles {
    numerator: u32,
    denominator: u32,
}

/// Reported ceiling state used for a flight category.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case", tag = "state", content = "feet")]
#[non_exhaustive]
pub enum Ceiling {
    /// Lowest broken, overcast, or vertical-visibility layer in feet AGL.
    Height(u32),
    /// The report establishes that no ceiling exists.
    NoCeiling,
}

impl StatuteMiles {
    /// Create a visibility value with a nonzero denominator.
    #[must_use]
    pub const fn new(numerator: u32, denominator: u32) -> Option<Self> {
        if denominator == 0 {
            None
        } else {
            Some(Self {
                numerator,
                denominator,
            })
        }
    }

    /// Get the numerator.
    #[must_use]
    pub const fn numerator(self) -> u32 {
        self.numerator
    }

    /// Get the denominator.
    #[must_use]
    pub const fn denominator(self) -> u32 {
        self.denominator
    }

    fn is_less_than(self, whole_miles: u32) -> bool {
        u64::from(self.numerator) < u64::from(whole_miles) * u64::from(self.denominator)
    }

    fn is_at_most(self, whole_miles: u32) -> bool {
        u64::from(self.numerator) <= u64::from(whole_miles) * u64::from(self.denominator)
    }
}

/// Flight category derived from ceiling and visibility.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum FlightCategory {
    /// Ceiling is above 3,000 ft and visibility is more than 5 statute miles.
    Vfr,
    /// Ceiling is 1,000 ft through 3,000 ft, or visibility is 3 through 5 statute miles.
    Mvfr,
    /// Ceiling is 500 ft through 999 ft, or visibility is 1 through less than 3 statute miles.
    Ifr,
    /// Ceiling is below 500 ft, or visibility is less than 1 statute mile.
    Lifr,
}

impl FlightCategory {
    /// Derive the most restrictive category from ceiling and visibility.
    ///
    /// FAA AIM paragraph 7-1-7 defines the thresholds.
    ///
    /// A ceiling of exactly 500 ft is IFR. A ceiling of exactly 1,000 ft is
    /// MVFR. A ceiling of exactly 3,000 ft is MVFR. Visibility of exactly 1
    /// statute mile is IFR. Visibility of exactly 3 or 5 statute miles is
    /// MVFR.
    #[must_use]
    pub fn from_conditions(ceiling: Ceiling, visibility: StatuteMiles) -> Self {
        core::cmp::max(
            Self::from_ceiling(ceiling),
            Self::from_visibility(visibility),
        )
    }

    fn from_ceiling(ceiling: Ceiling) -> Self {
        let Ceiling::Height(feet) = ceiling else {
            return Self::Vfr;
        };
        if feet < 500 {
            Self::Lifr
        } else if feet < 1_000 {
            Self::Ifr
        } else if feet <= 3_000 {
            Self::Mvfr
        } else {
            Self::Vfr
        }
    }

    fn from_visibility(miles: StatuteMiles) -> Self {
        if miles.is_less_than(1) {
            Self::Lifr
        } else if miles.is_less_than(3) {
            Self::Ifr
        } else if miles.is_at_most(5) {
            Self::Mvfr
        } else {
            Self::Vfr
        }
    }
}

/// One source-derived aviation weather text report.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct TextWeatherReport {
    report_type: FieldState<WeatherReportType>,
    station_id: FieldState<WeatherStationId>,
    report_time: FieldState<WeatherReportTime>,
    text: TimedField<String>,
    flight_category: FieldState<FlightCategory>,
}

impl TextWeatherReport {
    /// Create a text report from its source-derived fields.
    #[must_use]
    pub const fn new(
        report_type: FieldState<WeatherReportType>,
        station_id: FieldState<WeatherStationId>,
        report_time: FieldState<WeatherReportTime>,
        text: TimedField<String>,
        flight_category: FieldState<FlightCategory>,
    ) -> Self {
        Self {
            report_type,
            station_id,
            report_time,
            text,
            flight_category,
        }
    }

    /// Get the report type field.
    #[must_use]
    pub const fn report_type(&self) -> &FieldState<WeatherReportType> {
        &self.report_type
    }

    /// Get the station identity field.
    #[must_use]
    pub const fn station_id(&self) -> &FieldState<WeatherStationId> {
        &self.station_id
    }

    /// Get the report time field.
    #[must_use]
    pub const fn report_time(&self) -> &FieldState<WeatherReportTime> {
        &self.report_time
    }

    /// Get the unchanged source report text.
    #[must_use]
    pub const fn text(&self) -> &TimedField<String> {
        &self.text
    }

    /// Get the flight category field.
    #[must_use]
    pub const fn flight_category(&self) -> &FieldState<FlightCategory> {
        &self.flight_category
    }
}
