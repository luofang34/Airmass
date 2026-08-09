//! Present and absent weather field states.

use serde::{Deserialize, Serialize};

use crate::{MonotonicTime, ProductProvenance, QualityEvidence, TimeEvidence, UtcTime};

/// A source-derived value with its evidence.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct TimedField<T> {
    /// Source-derived value.
    pub value: T,
    /// Time evidence for the value.
    pub time: TimeEvidence,
    /// Quality evidence for the value.
    pub quality: QualityEvidence,
    /// Provenance for the value.
    pub provenance: ProductProvenance,
}

impl<T> TimedField<T> {
    /// Bind a value to its time, quality, and provenance.
    #[must_use]
    pub fn new(
        value: T,
        time: TimeEvidence,
        quality: QualityEvidence,
        provenance: ProductProvenance,
    ) -> Self {
        Self {
            value,
            time,
            quality,
            provenance,
        }
    }
}

/// Reason that a weather field has no value.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum AbsenceReason {
    /// The producer has not accepted evidence for the field.
    NotObserved,
    /// A source explicitly reported no value.
    SourceReportedUnavailable,
    /// An accepted value passed a validity or age limit.
    Expired,
    /// Candidate evidence failed an acceptance rule.
    Rejected,
    /// The field has no meaning for this product.
    NotApplicable,
    /// The producer cannot represent the source value.
    Unsupported,
    /// A declared capacity bound prevented retention.
    ResourceLimit,
    /// Source evidence cancelled the product.
    Cancelled,
    /// Source evidence is not effective at the evaluation time.
    NotEffective,
}

/// Evidence that supports an absent field state.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case", tag = "kind")]
pub enum AbsenceEvidence {
    /// No time or source evidence exists.
    Unknown,
    /// Source evidence supplies the absence.
    Source {
        /// Time evidence for the source report.
        time: TimeEvidence,
        /// Quality evidence for the source report.
        quality: QualityEvidence,
        /// Provenance for the source report.
        provenance: ProductProvenance,
    },
    /// A store lifecycle decision supplies the absence.
    Lifecycle {
        /// Civil UTC boundary that caused the decision.
        utc: Option<UtcTime>,
        /// Monotonic boundary that caused the decision.
        monotonic: Option<MonotonicTime>,
        /// Provenance of the value that caused the decision.
        provenance: Option<ProductProvenance>,
    },
}

/// One absent weather field.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct AbsentField {
    reason: AbsenceReason,
    evidence: AbsenceEvidence,
}

impl AbsentField {
    /// Create an absent field.
    #[must_use]
    pub const fn new(reason: AbsenceReason, evidence: AbsenceEvidence) -> Self {
        Self { reason, evidence }
    }

    /// Get the absence reason.
    #[must_use]
    pub const fn reason(&self) -> AbsenceReason {
        self.reason
    }

    /// Get the supporting evidence.
    #[must_use]
    pub const fn evidence(&self) -> &AbsenceEvidence {
        &self.evidence
    }
}

/// Present or absent state of one source-derived field.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case", tag = "state", content = "field")]
pub enum FieldState<T> {
    /// A value and its evidence exist.
    Present(TimedField<T>),
    /// An explicit reason replaces the value.
    Absent(AbsentField),
}

impl<T> FieldState<T> {
    /// Create a present field state.
    #[must_use]
    pub const fn present(field: TimedField<T>) -> Self {
        Self::Present(field)
    }

    /// Create an absent field state.
    #[must_use]
    pub const fn absent(field: AbsentField) -> Self {
        Self::Absent(field)
    }

    /// Borrow the present field when it exists.
    #[must_use]
    pub const fn as_present(&self) -> Option<&TimedField<T>> {
        match self {
            Self::Present(field) => Some(field),
            Self::Absent(_) => None,
        }
    }
}
