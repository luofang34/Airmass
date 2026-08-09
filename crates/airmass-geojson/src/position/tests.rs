use super::Wgs84Position;

#[test]
fn coordinate_ranges_are_closed() {
    assert!(Wgs84Position::new(-90.0, -180.0).is_some());
    assert!(Wgs84Position::new(90.0, 180.0).is_some());
    assert!(Wgs84Position::new(-90.1, 0.0).is_none());
    assert!(Wgs84Position::new(0.0, 180.1).is_none());
    assert!(Wgs84Position::new(f64::NAN, 0.0).is_none());
}
