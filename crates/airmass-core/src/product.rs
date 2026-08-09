//! Source-neutral weather products and cancellations.

use alloc::{string::String, vec::Vec};

use serde::{Deserialize, Serialize};

use crate::{DurationMillis, FieldState, TimedField, UtcInterval, UtcTime, WeatherValue};

/// Opaque source payload for a weather product.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct WeatherPayload {
    /// Media type or domain format name.
    pub media_type: String,
    /// Payload bytes without a source-protocol wrapper.
    pub bytes: Vec<u8>,
}

impl WeatherPayload {
    /// Create an opaque weather payload.
    #[must_use]
    pub fn new(media_type: impl Into<String>, bytes: Vec<u8>) -> Self {
        Self {
            media_type: media_type.into(),
            bytes,
        }
    }
}

/// Issue-time field for one product.
pub type ProductIssueTime = FieldState<UtcTime>;

/// One source-neutral weather product.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct WeatherProduct {
    payload: TimedField<WeatherPayload>,
    typed_value: FieldState<WeatherValue>,
    issue_time: ProductIssueTime,
    effective_at: FieldState<UtcTime>,
    valid_period: FieldState<UtcInterval>,
    maximum_age: FieldState<DurationMillis>,
}

impl WeatherProduct {
    /// Create a weather product from its source-derived fields.
    #[must_use]
    pub fn new(
        payload: TimedField<WeatherPayload>,
        typed_value: FieldState<WeatherValue>,
        issue_time: ProductIssueTime,
        effective_at: FieldState<UtcTime>,
        valid_period: FieldState<UtcInterval>,
        maximum_age: FieldState<DurationMillis>,
    ) -> Self {
        Self {
            payload,
            typed_value,
            issue_time,
            effective_at,
            valid_period,
            maximum_age,
        }
    }

    /// Get the product payload and its evidence.
    #[must_use]
    pub const fn payload(&self) -> &TimedField<WeatherPayload> {
        &self.payload
    }

    /// Get the typed weather value field.
    #[must_use]
    pub const fn typed_value(&self) -> &FieldState<WeatherValue> {
        &self.typed_value
    }

    /// Get the product issue-time field.
    #[must_use]
    pub const fn issue_time(&self) -> &ProductIssueTime {
        &self.issue_time
    }

    /// Get the product effective-time field.
    #[must_use]
    pub const fn effective_at(&self) -> &FieldState<UtcTime> {
        &self.effective_at
    }

    /// Get the product validity field.
    #[must_use]
    pub const fn valid_period(&self) -> &FieldState<UtcInterval> {
        &self.valid_period
    }

    /// Get the product maximum-age field.
    #[must_use]
    pub const fn maximum_age(&self) -> &FieldState<DurationMillis> {
        &self.maximum_age
    }
}

/// Source reason for a product cancellation.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case", tag = "kind", content = "name")]
#[non_exhaustive]
pub enum CancellationReason {
    /// The source replaced the product.
    Superseded,
    /// The source withdrew the product.
    Withdrawn,
    /// The source declared that the product is not valid.
    Invalid,
    /// The source supplied another named reason.
    Other(String),
}

/// Source-derived cancellation of one weather product.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct Cancellation {
    reason: TimedField<CancellationReason>,
    effective_at: FieldState<UtcTime>,
}

impl Cancellation {
    /// Create a cancellation.
    #[must_use]
    pub const fn new(
        reason: TimedField<CancellationReason>,
        effective_at: FieldState<UtcTime>,
    ) -> Self {
        Self {
            reason,
            effective_at,
        }
    }

    /// Get the source cancellation reason.
    #[must_use]
    pub const fn reason(&self) -> &TimedField<CancellationReason> {
        &self.reason
    }

    /// Get the cancellation effective-time field.
    #[must_use]
    pub const fn effective_at(&self) -> &FieldState<UtcTime> {
        &self.effective_at
    }
}
