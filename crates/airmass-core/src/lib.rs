//! Source-neutral weather ingress, lifecycle state, and immutable snapshots.
//!
//! The crate is `no_std + alloc` by default. It contains no source adapter,
//! clock, executor, channel, socket, lock, timer, command-line interface, or
//! renderer. A caller supplies time and calls the single-writer store
//! synchronously.

#![cfg_attr(not(feature = "std"), no_std)]

extern crate alloc;

mod advisory;
mod field;
mod identity;
mod ingress;
mod product;
mod provenance;
mod report;
mod schema;
mod snapshot;
mod store;
mod time;
mod value;

pub use advisory::{
    AdvisoryAltitude, AdvisoryAltitudeBand, AdvisoryAltitudeReference, AdvisoryGeometry,
    AdvisoryGeometryError, AdvisoryLine, AdvisoryPolygon, AdvisoryPosition, AdvisoryRing,
    AdvisoryValidationError, AdvisoryValidity, WeatherAdvisory, WeatherAdvisoryId,
    WeatherAdvisoryType,
};
pub use field::{AbsenceEvidence, AbsenceReason, AbsentField, FieldState, TimedField};
pub use identity::{
    ProducerInstanceId, ProductRevision, SnapshotRevision, SourceEpoch, SourceId,
    SourceProductRevision, WeatherProductId,
};
pub use ingress::{
    CURRENT_WEATHER_INGRESS_SCHEMA_VERSION, WeatherIngress, WeatherIngressEvent,
    WeatherIngressHeader,
};
pub use product::{
    Cancellation, CancellationReason, ProductIssueTime, WeatherPayload, WeatherProduct,
};
pub use provenance::{
    DeliveryPath, ProductProvenance, QualityEvidence, RawProductIdentity, SourceRef, WeatherOrigin,
};
pub use report::{
    Ceiling, FlightCategory, StatuteMiles, TextWeatherReport, WeatherReportTime, WeatherReportType,
    WeatherStationId,
};
pub use schema::WeatherSchemaError;
pub use snapshot::{
    CURRENT_WEATHER_SNAPSHOT_SCHEMA_VERSION, SelectionCriterion, SelectionExplanation,
    WeatherProductSnapshot, WeatherProductState, WeatherSnapshot, WeatherSnapshotEnvelope,
    WeatherSnapshotHandle, WeatherSnapshotRecord, WeatherSnapshotRecordHeader,
};
pub use store::{
    AcceptOutcome, AcceptStatus, AdvanceOutcome, SourcePriority, StoreConfig, StoreError,
    WeatherStore,
};
pub use time::{
    DeadlineSet, DurationMillis, EvaluationTime, IngressTimeBasis, MonotonicTime, SourceTime,
    SourceTimeKind, TimeBasis, TimeEvidence, TimeQuality, TimeUncertainty, UtcInterval, UtcTime,
};
pub use value::WeatherValue;
