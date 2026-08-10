//! Decodes text advisory geometry and altitude bands.

use alloc::{string::String, vec::Vec};

use airmass_core::{
    AbsenceReason, AdvisoryAltitude, AdvisoryAltitudeBand, AdvisoryAltitudeReference,
    AdvisoryGeometry, AdvisoryPolygon, AdvisoryPosition, AdvisoryRing, AdvisoryValidity,
    UtcInterval,
};

use super::{
    AdvisoryDecodeError, AdvisoryRecordMetadata, DecodedAdvisoryEvent, ResolvedIssueTime, advisory,
    advisory_type, time,
};
use crate::text_report::SourceEvidence;

pub(super) fn decode(
    product_id: u16,
    text: &str,
    metadata: AdvisoryRecordMetadata,
    issue: Option<ResolvedIssueTime>,
    evidence: &SourceEvidence,
) -> Result<DecodedAdvisoryEvent, AdvisoryDecodeError> {
    let identity = alloc::format!(
        "fisb-{}-{:02}-{}",
        product_id,
        metadata.report_year,
        metadata.report_number
    );
    decode_active(product_id, text, identity, issue, evidence)
}

pub(super) fn decode_legacy(
    product_id: u16,
    text: &str,
    identity: String,
    issue: Option<ResolvedIssueTime>,
    evidence: &SourceEvidence,
) -> Result<DecodedAdvisoryEvent, AdvisoryDecodeError> {
    if text.split_ascii_whitespace().any(is_cancellation_token) {
        return Ok(DecodedAdvisoryEvent::Cancellation {
            issue_time: issue.map(|value| value.utc),
        });
    }
    decode_active(product_id, text, identity, issue, evidence)
}

fn decode_active(
    product_id: u16,
    text: &str,
    identity: String,
    issue: Option<ResolvedIssueTime>,
    evidence: &SourceEvidence,
) -> Result<DecodedAdvisoryEvent, AdvisoryDecodeError> {
    let issue = issue.ok_or(AdvisoryDecodeError::InvalidTextField {
        field: "full issue time",
    })?;
    let valid_period = validity(text, issue)?;
    let geometry = polygon(text)?;
    let altitude_band = match band(text)? {
        Some(value) => evidence.present(value),
        None => evidence.absent(AbsenceReason::NotObserved),
    };
    let convective = text.contains("CONVECTIVE SIGMET");
    let kind = advisory_type(product_id, convective)
        .ok_or(AdvisoryDecodeError::UnsupportedProduct { product_id })?;
    Ok(DecodedAdvisoryEvent::Product {
        advisory: advisory(
            identity,
            kind,
            AdvisoryValidity::Period(valid_period),
            AdvisoryGeometry::Polygon(geometry),
            evidence.absent(AbsenceReason::NotObserved),
            altitude_band,
            evidence,
        ),
        issue_time: Some(issue.utc),
        valid_period,
    })
}

pub(super) fn legacy_identity(product_id: u16, text: &str) -> Result<String, AdvisoryDecodeError> {
    let marker = match product_id {
        2 | 3 | 22 | 23 => "SIGMET",
        4 | 24 => "AIRMET",
        6 | 26 => "WARNING",
        _ => {
            return Err(AdvisoryDecodeError::UnsupportedProduct { product_id });
        }
    };
    let tokens: Vec<&str> = text.split_ascii_whitespace().map(trim_token).collect();
    let Some(index) = tokens.iter().position(|token| *token == marker) else {
        return Err(AdvisoryDecodeError::InvalidTextField {
            field: "advisory identity",
        });
    };
    let issuer = tokens
        .get(index.saturating_sub(1))
        .copied()
        .unwrap_or("UNKNOWN");
    let designator = tokens
        .get(index + 1)
        .copied()
        .filter(|token| !token.is_empty() && *token != "VALID")
        .ok_or(AdvisoryDecodeError::InvalidTextField {
            field: "advisory identity",
        })?;
    Ok(alloc::format!(
        "fisb-advisory-{product_id}-{issuer}-{marker}-{designator}"
    ))
}

fn is_cancellation_token(token: &str) -> bool {
    matches!(trim_token(token), "CANCEL" | "CANCELLED" | "CNCL")
}

fn validity(text: &str, issue: ResolvedIssueTime) -> Result<UtcInterval, AdvisoryDecodeError> {
    let mut tokens = text.split_ascii_whitespace();
    while let Some(token) = tokens.next() {
        if token != "VALID" {
            continue;
        }
        let value = tokens
            .next()
            .ok_or(AdvisoryDecodeError::InvalidTextField { field: "validity" })?;
        return validity_token(value, issue);
    }
    Err(AdvisoryDecodeError::InvalidTextField { field: "validity" })
}

fn validity_token(
    value: &str,
    issue: ResolvedIssueTime,
) -> Result<UtcInterval, AdvisoryDecodeError> {
    let value = trim_token(value);
    let (start, end) = value
        .split_once('/')
        .ok_or(AdvisoryDecodeError::InvalidTextField { field: "validity" })?;
    let (start_day, start_hour, start_minute) = day_time(start)?;
    let (end_day, end_hour, end_minute) = day_time(end)?;
    let start = time::utc_for_day_near(
        issue.year,
        issue.month,
        issue.utc,
        start_day,
        start_hour,
        start_minute,
    )
    .map_err(AdvisoryDecodeError::Time)?;
    let end = time::utc_for_day_near(
        issue.year,
        issue.month,
        issue.utc,
        end_day,
        end_hour,
        end_minute,
    )
    .map_err(AdvisoryDecodeError::Time)?;
    let interval = UtcInterval::new(start, end);
    if !interval.has_valid_order() {
        return Err(AdvisoryDecodeError::InvalidValidity { start, end });
    }
    Ok(interval)
}

fn day_time(value: &str) -> Result<(u8, u8, u8), AdvisoryDecodeError> {
    if value.len() != 6 {
        return Err(AdvisoryDecodeError::InvalidTextField { field: "validity" });
    }
    let day = small_decimal(&value.as_bytes()[..2], "validity")?;
    let hour = small_decimal(&value.as_bytes()[2..4], "validity")?;
    let minute = small_decimal(&value.as_bytes()[4..6], "validity")?;
    Ok((day, hour, minute))
}

fn polygon(text: &str) -> Result<AdvisoryPolygon, AdvisoryDecodeError> {
    let mut positions = Vec::new();
    positions
        .try_reserve_exact(text.len() / 12)
        .map_err(|source| AdvisoryDecodeError::Allocation {
            operation: "text advisory positions",
            source,
        })?;
    let mut latitude = None;
    for raw in text.split_ascii_whitespace() {
        let token = trim_token(raw);
        if let Some(value) = latitude_token(token)? {
            latitude = Some(value);
            continue;
        }
        if let Some(lat) = latitude
            && let Some(lon) = longitude_token(token)?
        {
            latitude = None;
            positions
                .try_reserve(1)
                .map_err(|source| AdvisoryDecodeError::Allocation {
                    operation: "text advisory positions",
                    source,
                })?;
            positions.push(AdvisoryPosition::new(lat, lon)?);
        }
    }
    if positions.len() < 3 {
        return Err(AdvisoryDecodeError::InsufficientPositions {
            count: positions.len(),
            required: 3,
        });
    }
    if positions.first() != positions.last() {
        let first = positions[0];
        positions.push(first);
    }
    let ring = AdvisoryRing::new(positions)?;
    let mut rings = Vec::new();
    rings
        .try_reserve_exact(1)
        .map_err(|source| AdvisoryDecodeError::Allocation {
            operation: "text advisory rings",
            source,
        })?;
    rings.push(ring);
    AdvisoryPolygon::new(rings).map_err(AdvisoryDecodeError::Geometry)
}

fn latitude_token(token: &str) -> Result<Option<i32>, AdvisoryDecodeError> {
    coordinate(token, 2, b'N', b'S', 90, "latitude")
}

fn longitude_token(token: &str) -> Result<Option<i32>, AdvisoryDecodeError> {
    coordinate(token, 3, b'E', b'W', 180, "longitude")
}

fn coordinate(
    token: &str,
    degree_digits: usize,
    positive: u8,
    negative: u8,
    maximum: u16,
    field: &'static str,
) -> Result<Option<i32>, AdvisoryDecodeError> {
    let bytes = token.as_bytes();
    if bytes.len() != degree_digits + 3
        || !matches!(bytes[0], value if value == positive || value == negative)
    {
        return Ok(None);
    }
    let degrees = decimal(&bytes[1..1 + degree_digits], field)?;
    let minutes = decimal(&bytes[1 + degree_digits..], field)?;
    if degrees > maximum || minutes >= 60 || (degrees == maximum && minutes != 0) {
        return Err(AdvisoryDecodeError::InvalidTextField { field });
    }
    let magnitude = i32::from(degrees) * 1_000_000 + (i32::from(minutes) * 1_000_000 + 30) / 60;
    Ok(Some(if bytes[0] == negative {
        -magnitude
    } else {
        magnitude
    }))
}

fn band(text: &str) -> Result<Option<AdvisoryAltitudeBand>, AdvisoryDecodeError> {
    for raw in text.split_ascii_whitespace() {
        let token = trim_token(raw);
        let Some((base, top)) = token.split_once('/') else {
            continue;
        };
        let Some(base) = altitude(base, None)? else {
            continue;
        };
        let inherited = Some(base.reference());
        let Some(top) = altitude(top, inherited)? else {
            continue;
        };
        return AdvisoryAltitudeBand::new(base, top)
            .map(Some)
            .map_err(AdvisoryDecodeError::Geometry);
    }
    Ok(None)
}

fn altitude(
    value: &str,
    inherited: Option<AdvisoryAltitudeReference>,
) -> Result<Option<AdvisoryAltitude>, AdvisoryDecodeError> {
    if value == "SFC" {
        return Ok(Some(AdvisoryAltitude::new(
            0,
            AdvisoryAltitudeReference::Surface,
        )));
    }
    let (digits, reference) = if let Some(digits) = value.strip_prefix("FL") {
        (digits, AdvisoryAltitudeReference::FlightLevel)
    } else if let Some(reference) = inherited {
        (value, reference)
    } else {
        return Ok(None);
    };
    if digits.is_empty() || digits.len() > 3 {
        return Err(AdvisoryDecodeError::InvalidTextField {
            field: "altitude band",
        });
    }
    let hundreds = u32::from(decimal(digits.as_bytes(), "altitude band")?);
    Ok(Some(AdvisoryAltitude::new(hundreds * 100, reference)))
}

fn small_decimal(bytes: &[u8], field: &'static str) -> Result<u8, AdvisoryDecodeError> {
    u8::try_from(decimal(bytes, field)?)
        .map_err(|_| AdvisoryDecodeError::InvalidTextField { field })
}

fn decimal(bytes: &[u8], field: &'static str) -> Result<u16, AdvisoryDecodeError> {
    let mut value = 0_u16;
    for digit in bytes {
        if !digit.is_ascii_digit() {
            return Err(AdvisoryDecodeError::InvalidTextField { field });
        }
        value = value
            .checked_mul(10)
            .and_then(|current| current.checked_add(u16::from(*digit - b'0')))
            .ok_or(AdvisoryDecodeError::InvalidTextField { field })?;
    }
    Ok(value)
}

fn trim_token(value: &str) -> &str {
    value.trim_matches(|character: char| !character.is_ascii_alphanumeric() && character != '/')
}
