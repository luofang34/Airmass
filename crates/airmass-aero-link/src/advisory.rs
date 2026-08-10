//! Decodes FIS-B weather advisory records.

use alloc::{collections::TryReserveError, format, string::String, vec::Vec};

use aero_link::{FisBTime, Product8ReportStatus};
use airmass_core::{
    AdvisoryAltitude, AdvisoryGeometry, AdvisoryGeometryError, AdvisoryValidationError,
    AdvisoryValidity, FieldState, TimedField, UtcInterval, UtcTime, WeatherAdvisory,
    WeatherAdvisoryId, WeatherAdvisoryType,
};
use thiserror::Error;

use crate::text_report::SourceEvidence;

mod dlac;
mod graphical;
#[cfg(test)]
mod tests;
mod text;
mod time;

pub use time::AdvisoryTimeError;
pub(crate) use time::ResolvedIssueTime;

#[derive(Clone, Copy)]
pub(crate) struct AdvisoryRecordMetadata {
    pub(crate) record_format: u8,
    pub(crate) report_number: u16,
    pub(crate) report_year: u8,
    pub(crate) report_status: Option<Product8ReportStatus>,
}

#[derive(Clone, Copy)]
pub(crate) struct AdvisoryRecordInput<'a> {
    pub(crate) product_id: u16,
    pub(crate) payload: &'a [u8],
    pub(crate) decoded_text: Option<&'a str>,
    pub(crate) metadata: AdvisoryRecordMetadata,
}

#[allow(
    clippy::large_enum_variant,
    reason = "the transient decode event does not need another allocation"
)]
pub(crate) enum DecodedAdvisoryEvent {
    Product {
        advisory: WeatherAdvisory,
        issue_time: Option<UtcTime>,
        valid_period: UtcInterval,
    },
    Cancellation {
        issue_time: Option<UtcTime>,
    },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum AdvisoryEncoding {
    Text,
    DlacText,
    Twgo,
}

pub(crate) const fn advisory_encoding(product_id: u16) -> Option<AdvisoryEncoding> {
    match product_id {
        2 | 3 | 4 | 6 => Some(AdvisoryEncoding::Text),
        22 | 23 | 24 | 26 => Some(AdvisoryEncoding::DlacText),
        11 | 12 | 14 | 15 => Some(AdvisoryEncoding::Twgo),
        _ => None,
    }
}

pub(crate) fn is_advisory_product(product_id: u16) -> bool {
    advisory_encoding(product_id).is_some()
}

pub(crate) fn resolve_issue_time(
    report_year: u8,
    product_time: Option<FisBTime>,
) -> Result<Option<ResolvedIssueTime>, AdvisoryDecodeError> {
    time::resolve_issue_time(report_year, product_time).map_err(AdvisoryDecodeError::Time)
}

pub(crate) fn resolve_legacy_issue_time(
    reference: UtcTime,
    product_time: Option<FisBTime>,
) -> Result<Option<ResolvedIssueTime>, AdvisoryDecodeError> {
    time::resolve_issue_time_near(reference, product_time).map_err(AdvisoryDecodeError::Time)
}

pub(crate) fn product_identity(product_id: u16, metadata: AdvisoryRecordMetadata) -> String {
    format!(
        "fisb-advisory-{product_id}-{:02}-{}",
        metadata.report_year, metadata.report_number
    )
}

pub(crate) fn decode_advisory(
    input: AdvisoryRecordInput<'_>,
    issue: Option<ResolvedIssueTime>,
    evidence: &SourceEvidence,
) -> Result<DecodedAdvisoryEvent, AdvisoryDecodeError> {
    match input.metadata.record_format {
        2 => decode_text(input, issue, evidence),
        8 => {
            let kind = graphical_advisory_type(input.product_id).ok_or(
                AdvisoryDecodeError::MissingPairedText {
                    product_id: input.product_id,
                    report_year: input.metadata.report_year,
                    report_number: input.metadata.report_number,
                },
            )?;
            graphical::decode(
                core::slice::from_ref(&input),
                issue,
                evidence.timed(kind),
                evidence,
            )
        }
        record_format => Err(AdvisoryDecodeError::UnsupportedRecordFormat { record_format }),
    }
}

pub(crate) fn decode_graphical_advisory(
    inputs: &[AdvisoryRecordInput<'_>],
    issue: Option<ResolvedIssueTime>,
    advisory_type: TimedField<WeatherAdvisoryType>,
    evidence: &SourceEvidence,
) -> Result<DecodedAdvisoryEvent, AdvisoryDecodeError> {
    graphical::decode(inputs, issue, advisory_type, evidence)
}

pub(crate) fn decode_legacy_payload(
    product_id: u16,
    payload: &[u8],
) -> Result<String, AdvisoryDecodeError> {
    match advisory_encoding(product_id) {
        Some(AdvisoryEncoding::Text) => {
            let text = core::str::from_utf8(payload)
                .map_err(|_| AdvisoryDecodeError::InvalidTextEncoding)?;
            let mut output = String::new();
            output.try_reserve_exact(text.len()).map_err(|source| {
                AdvisoryDecodeError::Allocation {
                    operation: "legacy advisory text",
                    source,
                }
            })?;
            output.push_str(text.trim_matches(char::from(0)));
            Ok(output)
        }
        Some(AdvisoryEncoding::DlacText) => dlac::decode(payload),
        _ => Err(AdvisoryDecodeError::UnsupportedProduct { product_id }),
    }
}

pub(crate) fn legacy_product_identity(
    product_id: u16,
    text: &str,
) -> Result<String, AdvisoryDecodeError> {
    text::legacy_identity(product_id, text)
}

pub(crate) fn decode_legacy_advisory(
    product_id: u16,
    text: &str,
    identity: String,
    issue: Option<ResolvedIssueTime>,
    evidence: &SourceEvidence,
) -> Result<DecodedAdvisoryEvent, AdvisoryDecodeError> {
    text::decode_legacy(product_id, text, identity, issue, evidence)
}

fn decode_text(
    input: AdvisoryRecordInput<'_>,
    issue: Option<ResolvedIssueTime>,
    evidence: &SourceEvidence,
) -> Result<DecodedAdvisoryEvent, AdvisoryDecodeError> {
    match input.metadata.report_status {
        Some(Product8ReportStatus::Cancelled) => Ok(DecodedAdvisoryEvent::Cancellation {
            issue_time: issue.map(|value| value.utc),
        }),
        Some(Product8ReportStatus::Active) => {
            let decoded = input
                .decoded_text
                .ok_or(AdvisoryDecodeError::MissingDecodedText)?;
            text::decode(input.product_id, decoded, input.metadata, issue, evidence)
        }
        None => Err(AdvisoryDecodeError::MissingTextStatus),
    }
}

pub(super) fn advisory(
    identity: String,
    advisory_type: WeatherAdvisoryType,
    validity: AdvisoryValidity,
    geometry: AdvisoryGeometry,
    altitude: FieldState<AdvisoryAltitude>,
    altitude_band: FieldState<airmass_core::AdvisoryAltitudeBand>,
    evidence: &SourceEvidence,
) -> WeatherAdvisory {
    advisory_with_type(
        identity,
        evidence.timed(advisory_type),
        validity,
        geometry,
        altitude,
        altitude_band,
        evidence,
    )
}

#[allow(clippy::too_many_arguments)]
pub(super) fn advisory_with_type(
    identity: String,
    advisory_type: TimedField<WeatherAdvisoryType>,
    validity: AdvisoryValidity,
    geometry: AdvisoryGeometry,
    altitude: FieldState<AdvisoryAltitude>,
    altitude_band: FieldState<airmass_core::AdvisoryAltitudeBand>,
    evidence: &SourceEvidence,
) -> WeatherAdvisory {
    WeatherAdvisory::new(
        evidence.present(WeatherAdvisoryId::new(identity)),
        FieldState::Present(advisory_type),
        evidence.present(validity),
        evidence.present(geometry),
        altitude,
        altitude_band,
    )
}

pub(super) const fn advisory_type(
    product_id: u16,
    convective_text: bool,
) -> Option<WeatherAdvisoryType> {
    match product_id {
        2 | 22 => Some(WeatherAdvisoryType::Sigmet),
        3 | 23 => Some(WeatherAdvisoryType::ConvectiveSigmet),
        4 | 24 | 11 => Some(WeatherAdvisoryType::Airmet),
        6 | 26 => Some(WeatherAdvisoryType::AviationWeatherWarning),
        12 if convective_text => Some(WeatherAdvisoryType::ConvectiveSigmet),
        12 => Some(WeatherAdvisoryType::Sigmet),
        14 => Some(WeatherAdvisoryType::GAirmet),
        15 => Some(WeatherAdvisoryType::CenterWeatherAdvisory),
        _ => None,
    }
}

pub(super) const fn graphical_advisory_type(product_id: u16) -> Option<WeatherAdvisoryType> {
    match product_id {
        11 => Some(WeatherAdvisoryType::Airmet),
        14 => Some(WeatherAdvisoryType::GAirmet),
        15 => Some(WeatherAdvisoryType::CenterWeatherAdvisory),
        _ => None,
    }
}

pub(super) fn one_advisory(
    value: WeatherAdvisory,
) -> Result<Vec<WeatherAdvisory>, AdvisoryDecodeError> {
    let mut values = Vec::new();
    values
        .try_reserve_exact(1)
        .map_err(|source| AdvisoryDecodeError::Allocation {
            operation: "typed advisory list",
            source,
        })?;
    values.push(value);
    Ok(values)
}

/// Failure to decode one FIS-B weather advisory record.
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum AdvisoryDecodeError {
    /// The record format is not a supported advisory format.
    #[error("FIS-B advisory record format {record_format} is not supported")]
    UnsupportedRecordFormat {
        /// Record format value.
        record_format: u8,
    },
    /// A text record has no active or cancelled status.
    #[error("FIS-B advisory text record has no report status")]
    MissingTextStatus,
    /// An active text record has no decoded text.
    #[error("FIS-B advisory text record has no decoded text")]
    MissingDecodedText,
    /// A Product 12 graphical record has no matching text record.
    #[error(
        "FIS-B Product {product_id} graphical report {report_year:02}-{report_number} has no matching text record"
    )]
    MissingPairedText {
        /// Numeric FIS-B product identifier.
        product_id: u16,
        /// Two-digit report year.
        report_year: u8,
        /// Report number.
        report_number: u16,
    },
    /// A legacy text payload is not valid for its declared encoding.
    #[error("FIS-B legacy advisory text encoding is invalid")]
    InvalidTextEncoding,
    /// A legacy advisory needs caller-supplied civil UTC.
    #[error("FIS-B legacy advisory needs caller-supplied civil UTC")]
    MissingCivilUtc,
    /// The product identifier has no advisory type.
    #[error("FIS-B product {product_id} has no advisory type")]
    UnsupportedProduct {
        /// Numeric FIS-B product identifier.
        product_id: u16,
    },
    /// A fixed record field is incomplete.
    #[error("FIS-B advisory field {field} needs {required} bytes; {remaining} remain")]
    Truncated {
        /// Name of the incomplete field.
        field: &'static str,
        /// Required byte count.
        required: usize,
        /// Available byte count.
        remaining: usize,
    },
    /// A graphical field has an unsupported value.
    #[error("FIS-B advisory graphical field {field} has unsupported value {value}")]
    UnsupportedGraphicalField {
        /// Name of the rejected field.
        field: &'static str,
        /// Rejected field value.
        value: u8,
    },
    /// A graphical record has unexpected trailing bytes.
    #[error("FIS-B advisory graphical record has {remaining} trailing bytes")]
    TrailingGraphicalBytes {
        /// Unexpected byte count.
        remaining: usize,
    },
    /// No graphical record was supplied for a report.
    #[error("FIS-B advisory graphical report has no record")]
    EmptyGraphicalRecords,
    /// A report has more graphical fragments than its identifier can represent.
    #[error("FIS-B advisory graphical report has {count} records; at most 15 are supported")]
    GraphicalRecordCount {
        /// Supplied graphical record count.
        count: usize,
    },
    /// Graphical fragments do not form one zero-based sequence.
    #[error("FIS-B advisory graphical record sequence expected {expected} and found {found}")]
    GraphicalRecordSequence {
        /// Required next overlay record identifier.
        expected: u8,
        /// Supplied overlay record identifier.
        found: u8,
    },
    /// Graphical fragments disagree on a shared field.
    #[error("FIS-B advisory graphical records have inconsistent {field}")]
    InconsistentGraphicalField {
        /// Name of the inconsistent field.
        field: &'static str,
    },
    /// The combined graphical vertex count does not fit in memory indexing.
    #[error("FIS-B advisory graphical vertex count overflowed")]
    GraphicalVertexCountOverflow,
    /// A validity interval has no positive duration.
    #[error("FIS-B advisory validity starts at {start:?} and ends at {end:?}")]
    InvalidValidity {
        /// Validity start.
        start: UtcTime,
        /// Validity end.
        end: UtcTime,
    },
    /// A center weather advisory is valid for more than two hours.
    #[error("FIS-B center weather advisory validity from {start:?} to {end:?} exceeds two hours")]
    CwaDurationTooLong {
        /// Validity start.
        start: UtcTime,
        /// Validity end.
        end: UtcTime,
    },
    /// A center weather advisory starts more than two hours after its issue time.
    #[error(
        "FIS-B center weather advisory starts at {start:?}, more than two hours after {issue:?}"
    )]
    CwaStartTooLate {
        /// Product issue time.
        issue: UtcTime,
        /// Validity start.
        start: UtcTime,
    },
    /// A text field is absent or malformed.
    #[error("FIS-B advisory text does not contain a valid {field}")]
    InvalidTextField {
        /// Name of the invalid field.
        field: &'static str,
    },
    /// The source geometry has too few positions.
    #[error("FIS-B advisory geometry has {count} positions; at least {required} are required")]
    InsufficientPositions {
        /// Supplied position count.
        count: usize,
        /// Required position count.
        required: usize,
    },
    /// Source coordinates do not form a valid advisory geometry.
    #[error("FIS-B advisory geometry is invalid")]
    Geometry(#[from] AdvisoryGeometryError),
    /// Source-derived advisory structure is invalid.
    #[error("FIS-B advisory structure is invalid")]
    Validation(#[from] AdvisoryValidationError),
    /// A source date is invalid.
    #[error("FIS-B advisory time is invalid")]
    Time(#[source] AdvisoryTimeError),
    /// A bounded output allocation failed.
    #[error("allocation failed for {operation}")]
    Allocation {
        /// Output operation that requested memory.
        operation: &'static str,
        /// Allocator error.
        #[source]
        source: TryReserveError,
    },
}
