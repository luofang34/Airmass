#![allow(clippy::expect_used)]

use alloc::vec;

use super::*;

#[test]
fn polygon_requires_a_closed_ring() {
    let first = AdvisoryPosition::new(10_000_000, -20_000_000).expect("position must be valid");
    let second = AdvisoryPosition::new(11_000_000, -20_000_000).expect("position must be valid");
    let third = AdvisoryPosition::new(11_000_000, -21_000_000).expect("position must be valid");

    let error =
        AdvisoryRing::new(vec![first, second, third, second]).expect_err("an open ring must fail");

    assert_eq!(error, AdvisoryGeometryError::RingNotClosed);
}

#[test]
fn altitude_band_keeps_both_source_limits() {
    let base = AdvisoryAltitude::new(15_000, AdvisoryAltitudeReference::FlightLevel);
    let top = AdvisoryAltitude::new(26_000, AdvisoryAltitudeReference::FlightLevel);

    let band = AdvisoryAltitudeBand::new(base, top).expect("the band order must be valid");

    assert_eq!(band.base(), base);
    assert_eq!(band.top(), top);
}
