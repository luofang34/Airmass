//! Renderer-neutral weather features from Airmass snapshots.
//!
//! [`map_snapshot_transition`] converts applicable weather values into typed
//! feature changes. The optional `geojson` feature adds RFC 7946 encoding.
//! The crate has no renderer, source adapter, clock, task, or lock.

#![no_std]

extern crate alloc;

mod feature;
#[cfg(feature = "geojson")]
mod geojson;
mod position;

pub use feature::{
    AdvisoryShapeFeature, AdvisoryShapeGeometry, AdvisoryShapeLine, AdvisoryShapeRing,
    FeatureDelta, FeatureRemovalReason, StationPositionResolver, TextReportFeature, WeatherFeature,
    WeatherFeatureId, WeatherFeatureKey, WeatherFeatureRevisions, map_snapshot_transition,
};
#[cfg(feature = "geojson")]
pub use geojson::{GeoJsonError, encode_feature_delta};
pub use position::Wgs84Position;
