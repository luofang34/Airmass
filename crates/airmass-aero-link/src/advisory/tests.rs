#![allow(clippy::expect_used)]

use aero_link::FisBTime;
use airmass_core::{UtcTime, WeatherAdvisoryType};

use super::*;

#[test]
fn each_advisory_product_has_one_encoding_and_type() {
    for (product_id, encoding, expected_type) in [
        (2, AdvisoryEncoding::Text, WeatherAdvisoryType::Sigmet),
        (22, AdvisoryEncoding::DlacText, WeatherAdvisoryType::Sigmet),
        (
            3,
            AdvisoryEncoding::Text,
            WeatherAdvisoryType::ConvectiveSigmet,
        ),
        (
            23,
            AdvisoryEncoding::DlacText,
            WeatherAdvisoryType::ConvectiveSigmet,
        ),
        (4, AdvisoryEncoding::Text, WeatherAdvisoryType::Airmet),
        (24, AdvisoryEncoding::DlacText, WeatherAdvisoryType::Airmet),
        (
            6,
            AdvisoryEncoding::Text,
            WeatherAdvisoryType::AviationWeatherWarning,
        ),
        (
            26,
            AdvisoryEncoding::DlacText,
            WeatherAdvisoryType::AviationWeatherWarning,
        ),
        (11, AdvisoryEncoding::Twgo, WeatherAdvisoryType::Airmet),
        (12, AdvisoryEncoding::Twgo, WeatherAdvisoryType::Sigmet),
        (14, AdvisoryEncoding::Twgo, WeatherAdvisoryType::GAirmet),
        (
            15,
            AdvisoryEncoding::Twgo,
            WeatherAdvisoryType::CenterWeatherAdvisory,
        ),
    ] {
        assert_eq!(advisory_encoding(product_id), Some(encoding));
        assert_eq!(advisory_type(product_id, false), Some(expected_type));
    }
}

#[test]
fn legacy_clock_time_uses_the_nearest_caller_date() {
    let reference = UtcTime::from_unix_millis(1_786_330_800_000);
    for (second, expected_millis) in [(None, 1_786_331_280_000), (Some(30), 1_786_331_310_000)] {
        let source = FisBTime {
            month: None,
            day: None,
            hour: 3,
            minute: 8,
            second,
        };
        let resolved = resolve_legacy_issue_time(reference, Some(source))
            .expect("the source clock must be valid")
            .expect("the source clock must resolve");
        assert_eq!(resolved.utc, UtcTime::from_unix_millis(expected_millis));
    }
}
