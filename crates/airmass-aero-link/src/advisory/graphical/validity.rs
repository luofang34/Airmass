//! Validates TWGO graphical advisory applicability.

use airmass_core::{AdvisoryValidity, UtcInterval, UtcTime};

use super::super::{AdvisoryDecodeError, ResolvedIssueTime, time};

const CWA_MAX_MILLIS: i64 = 2 * 60 * 60 * 1_000;

pub(super) fn decode(
    report_year: u8,
    bytes: &[u8],
    product_id: u16,
    issue: Option<ResolvedIssueTime>,
) -> Result<AdvisoryValidity, AdvisoryDecodeError> {
    let start_year = 2_000_i32 + i32::from(report_year);
    let start = decode_date(start_year, &bytes[..4], issue)?;
    let end_year = if issue.is_none() && bytes[4] < bytes[0] {
        start_year + 1
    } else {
        start_year
    };
    let end = decode_date(end_year, &bytes[4..8], issue)?;
    if start == end && product_id == 14 {
        return Ok(AdvisoryValidity::Point(start));
    }
    let interval = UtcInterval::new(start, end);
    if !interval.has_valid_order() {
        return Err(AdvisoryDecodeError::InvalidValidity { start, end });
    }
    validate_cwa(product_id, issue, interval)?;
    Ok(AdvisoryValidity::Period(interval))
}

fn validate_cwa(
    product_id: u16,
    issue: Option<ResolvedIssueTime>,
    validity: UtcInterval,
) -> Result<(), AdvisoryDecodeError> {
    if product_id != 15 {
        return Ok(());
    }
    let duration = validity
        .end
        .as_unix_millis()
        .checked_sub(validity.start.as_unix_millis())
        .ok_or(AdvisoryDecodeError::CwaDurationTooLong {
            start: validity.start,
            end: validity.end,
        })?;
    if duration > CWA_MAX_MILLIS {
        return Err(AdvisoryDecodeError::CwaDurationTooLong {
            start: validity.start,
            end: validity.end,
        });
    }
    if let Some(issue) = issue {
        validate_cwa_start(issue.utc, validity.start)?;
    }
    Ok(())
}

fn validate_cwa_start(issue: UtcTime, start: UtcTime) -> Result<(), AdvisoryDecodeError> {
    if start <= issue {
        return Ok(());
    }
    let delay = start
        .as_unix_millis()
        .checked_sub(issue.as_unix_millis())
        .ok_or(AdvisoryDecodeError::CwaStartTooLate { issue, start })?;
    if delay > CWA_MAX_MILLIS {
        return Err(AdvisoryDecodeError::CwaStartTooLate { issue, start });
    }
    Ok(())
}

fn decode_date(
    year: i32,
    bytes: &[u8],
    issue: Option<ResolvedIssueTime>,
) -> Result<UtcTime, AdvisoryDecodeError> {
    issue
        .map_or_else(
            || time::utc_from_parts(year, bytes[0], bytes[1], bytes[2], bytes[3], 0),
            |reference| {
                time::utc_for_date_near(reference.utc, bytes[0], bytes[1], bytes[2], bytes[3])
            },
        )
        .map_err(AdvisoryDecodeError::Time)
}
