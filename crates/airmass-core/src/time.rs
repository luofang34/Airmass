//! Civil UTC and monotonic time evidence.

use serde::{Deserialize, Serialize};

/// A caller-owned monotonic clock value in microseconds.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(transparent)]
pub struct MonotonicTime(u64);

impl MonotonicTime {
    /// Create a monotonic clock value.
    #[must_use]
    pub const fn from_micros(micros: u64) -> Self {
        Self(micros)
    }

    /// Get the clock value in microseconds.
    #[must_use]
    pub const fn as_micros(self) -> u64 {
        self.0
    }

    pub(crate) fn checked_add(self, duration: DurationMillis) -> Option<Self> {
        duration
            .0
            .checked_mul(1_000)
            .and_then(|micros| self.0.checked_add(micros))
            .map(Self)
    }
}

/// A civil UTC value in milliseconds since the Unix epoch.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(transparent)]
pub struct UtcTime(i64);

impl UtcTime {
    /// Create a civil UTC value.
    #[must_use]
    pub const fn from_unix_millis(millis: i64) -> Self {
        Self(millis)
    }

    /// Get milliseconds since the Unix epoch.
    #[must_use]
    pub const fn as_unix_millis(self) -> i64 {
        self.0
    }
}

/// A duration in milliseconds.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(transparent)]
pub struct DurationMillis(u64);

impl DurationMillis {
    /// Create a duration in milliseconds.
    #[must_use]
    pub const fn new(millis: u64) -> Self {
        Self(millis)
    }

    /// Get the duration in milliseconds.
    #[must_use]
    pub const fn get(self) -> u64 {
        self.0
    }
}

/// Civil UTC and monotonic time supplied for one store operation.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct EvaluationTime {
    /// Civil UTC for validity decisions.
    pub utc: UtcTime,
    /// Monotonic time for order and age decisions.
    pub monotonic: MonotonicTime,
}

impl EvaluationTime {
    /// Create an evaluation time.
    #[must_use]
    pub const fn new(utc: UtcTime, monotonic: MonotonicTime) -> Self {
        Self { utc, monotonic }
    }
}

/// A half-open civil UTC interval.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct UtcInterval {
    /// First valid instant.
    pub start: UtcTime,
    /// First invalid instant after the interval.
    pub end: UtcTime,
}

impl UtcInterval {
    /// Create a half-open interval.
    #[must_use]
    pub const fn new(start: UtcTime, end: UtcTime) -> Self {
        Self { start, end }
    }

    /// Test if the interval has a valid order.
    #[must_use]
    pub fn has_valid_order(self) -> bool {
        self.start < self.end
    }

    /// Test if the interval contains a civil UTC value.
    #[must_use]
    pub fn contains(self, value: UtcTime) -> bool {
        self.start <= value && value < self.end
    }
}

/// Meaning of a source time value.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum SourceTimeKind {
    /// Time when a source made an observation.
    Observation,
    /// Time when a source issued a product.
    Issue,
    /// Time assigned to a source product.
    Product,
}

/// Method that established a source time.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum TimeBasis {
    /// The source supplied civil UTC directly.
    SourceUtc,
    /// A clock correspondence supplied civil UTC.
    ClockCorrespondence,
    /// The source supplied a relative value that an adapter resolved.
    AdapterResolved,
}

/// Declared quality of a source time.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum TimeQuality {
    /// The source declares an exact value.
    Exact,
    /// The source declares an estimated value.
    Estimated,
    /// The source value has limited resolution.
    Coarse,
    /// The source supplies no time quality value.
    Unavailable,
}

/// Error bound for a mapped source time.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case", tag = "state", content = "micros")]
pub enum TimeUncertainty {
    /// The source gives no error bound.
    Unknown,
    /// The error is at most this many microseconds.
    Bounded(u64),
}

/// Method that established a monotonic ingress stamp.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum IngressTimeBasis {
    /// The runtime stamped local input.
    LocalIngress,
    /// The runtime mapped a source time to its monotonic clock.
    MappedSourceTime,
    /// The method is not known.
    Unknown,
}

/// Source time evidence for one field.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case", tag = "state")]
pub enum SourceTime {
    /// No valid source time exists.
    Unknown,
    /// A source time and its supporting information exist.
    Known {
        /// Source time value.
        value: UtcTime,
        /// Meaning of the time value.
        kind: SourceTimeKind,
        /// Method that established the time.
        basis: TimeBasis,
        /// Declared time quality.
        quality: TimeQuality,
        /// Error bound for the time value.
        uncertainty: TimeUncertainty,
    },
}

/// Time evidence for one source-derived field.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct TimeEvidence {
    /// Monotonic stamp recorded at runtime ingress.
    pub ingress_monotonic: MonotonicTime,
    /// Method that established the ingress stamp.
    pub ingress_basis: IngressTimeBasis,
    /// Error bound for the ingress stamp.
    pub ingress_uncertainty: TimeUncertainty,
    /// Source observation or product time.
    pub source: SourceTime,
}

impl TimeEvidence {
    /// Create time evidence.
    #[must_use]
    pub const fn new(ingress_monotonic: MonotonicTime, source: SourceTime) -> Self {
        Self {
            ingress_monotonic,
            ingress_basis: IngressTimeBasis::Unknown,
            ingress_uncertainty: TimeUncertainty::Unknown,
            source,
        }
    }

    /// Create time evidence with explicit ingress-clock evidence.
    #[must_use]
    pub const fn with_ingress_evidence(
        ingress_monotonic: MonotonicTime,
        ingress_basis: IngressTimeBasis,
        ingress_uncertainty: TimeUncertainty,
        source: SourceTime,
    ) -> Self {
        Self {
            ingress_monotonic,
            ingress_basis,
            ingress_uncertainty,
            source,
        }
    }
}

/// Next times when a store result can change.
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct DeadlineSet {
    /// Next civil UTC deadline.
    pub utc: Option<UtcTime>,
    /// Next monotonic deadline.
    pub monotonic: Option<MonotonicTime>,
}

impl DeadlineSet {
    pub(crate) fn include_utc(&mut self, value: UtcTime) {
        self.utc = Some(self.utc.map_or(value, |current| current.min(value)));
    }

    pub(crate) fn include_monotonic(&mut self, value: MonotonicTime) {
        self.monotonic = Some(self.monotonic.map_or(value, |current| current.min(value)));
    }
}
