use super::{Ceiling, FlightCategory, StatuteMiles, WeatherReportTime};

#[test]
fn ceiling_threshold_boundaries_are_exact() {
    let visibility = StatuteMiles::new(10, 1).expect("the denominator must be nonzero");
    for (ceiling, expected) in [
        (499, FlightCategory::Lifr),
        (500, FlightCategory::Ifr),
        (999, FlightCategory::Ifr),
        (1_000, FlightCategory::Mvfr),
        (3_000, FlightCategory::Mvfr),
        (3_001, FlightCategory::Vfr),
    ] {
        assert_eq!(
            FlightCategory::from_conditions(Ceiling::Height(ceiling), visibility),
            expected,
            "ceiling {ceiling}"
        );
    }
}

#[test]
fn visibility_threshold_boundaries_are_exact() {
    for (numerator, denominator, expected) in [
        (99, 100, FlightCategory::Lifr),
        (1, 1, FlightCategory::Ifr),
        (299, 100, FlightCategory::Ifr),
        (3, 1, FlightCategory::Mvfr),
        (5, 1, FlightCategory::Mvfr),
        (501, 100, FlightCategory::Vfr),
    ] {
        let visibility =
            StatuteMiles::new(numerator, denominator).expect("the denominator must be nonzero");
        assert_eq!(
            FlightCategory::from_conditions(Ceiling::NoCeiling, visibility),
            expected,
            "visibility {numerator}/{denominator}"
        );
    }
}

#[test]
fn the_most_restrictive_measurement_controls_the_category() {
    let visibility = StatuteMiles::new(10, 1).expect("the denominator must be nonzero");
    assert_eq!(
        FlightCategory::from_conditions(Ceiling::Height(400), visibility),
        FlightCategory::Lifr
    );
}

#[test]
fn a_report_time_rejects_each_out_of_range_field() {
    assert_eq!(WeatherReportTime::new(0, 12, 30), None);
    assert_eq!(WeatherReportTime::new(32, 12, 30), None);
    assert_eq!(WeatherReportTime::new(1, 24, 30), None);
    assert_eq!(WeatherReportTime::new(1, 12, 60), None);
    assert_eq!(
        WeatherReportTime::new(31, 23, 59).map(|time| (
            time.day_of_month(),
            time.hour(),
            time.minute()
        )),
        Some((31, 23, 59))
    );
}
