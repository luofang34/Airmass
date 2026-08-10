//! Source-neutral typed weather values.

use alloc::vec::Vec;

use serde::{Deserialize, Serialize};

use crate::{TextWeatherReport, WeatherAdvisory};

/// Typed weather data derived from one source product.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case", tag = "kind", content = "value")]
#[non_exhaustive]
pub enum WeatherValue {
    /// Text reports in source order.
    TextReports(Vec<TextWeatherReport>),
    /// Weather advisories in source order.
    Advisories(Vec<WeatherAdvisory>),
}

impl WeatherValue {
    /// Get the text reports when this is a text value.
    #[must_use]
    pub fn text_reports(&self) -> Option<&[TextWeatherReport]> {
        match self {
            Self::TextReports(reports) => Some(reports),
            Self::Advisories(_) => None,
        }
    }

    /// Get the advisories when this is an advisory value.
    #[must_use]
    pub fn advisories(&self) -> Option<&[WeatherAdvisory]> {
        match self {
            Self::Advisories(advisories) => Some(advisories),
            Self::TextReports(_) => None,
        }
    }
}
