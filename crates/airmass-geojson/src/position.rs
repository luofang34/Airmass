//! WGS84 positions for weather features.

/// One position in the WGS84 coordinate system.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Wgs84Position {
    latitude_deg: f64,
    longitude_deg: f64,
}

#[cfg(test)]
mod tests;

impl Wgs84Position {
    /// Create a position when both coordinates are finite and in range.
    #[must_use]
    pub fn new(latitude_deg: f64, longitude_deg: f64) -> Option<Self> {
        if latitude_deg.is_finite()
            && longitude_deg.is_finite()
            && (-90.0..=90.0).contains(&latitude_deg)
            && (-180.0..=180.0).contains(&longitude_deg)
        {
            Some(Self {
                latitude_deg,
                longitude_deg,
            })
        } else {
            None
        }
    }

    /// Get latitude in degrees.
    #[must_use]
    pub const fn latitude_deg(self) -> f64 {
        self.latitude_deg
    }

    /// Get longitude in degrees.
    #[must_use]
    pub const fn longitude_deg(self) -> f64 {
        self.longitude_deg
    }
}
