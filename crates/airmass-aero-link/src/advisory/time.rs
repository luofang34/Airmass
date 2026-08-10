//! Calendar conversion for advisory source times.

use aero_link::FisBTime;
use airmass_core::UtcTime;
use thiserror::Error;

const MILLIS_PER_SECOND: i64 = 1_000;
const SECONDS_PER_DAY: i64 = 86_400;

#[derive(Clone, Copy, Debug)]
pub(crate) struct ResolvedIssueTime {
    pub(crate) utc: UtcTime,
    pub(crate) year: i32,
    pub(crate) month: u8,
}

pub(super) fn resolve_issue_time(
    report_year: u8,
    source: Option<FisBTime>,
) -> Result<Option<ResolvedIssueTime>, AdvisoryTimeError> {
    let Some(source) = source else {
        return Ok(None);
    };
    let (Some(month), Some(day)) = (source.month, source.day) else {
        return Ok(None);
    };
    let year = 2_000_i32 + i32::from(report_year);
    let utc = utc_from_parts(
        year,
        month,
        day,
        source.hour,
        source.minute,
        source.second.unwrap_or(0),
    )?;
    Ok(Some(ResolvedIssueTime { utc, year, month }))
}

pub(super) fn resolve_issue_time_near(
    reference: UtcTime,
    source: Option<FisBTime>,
) -> Result<Option<ResolvedIssueTime>, AdvisoryTimeError> {
    let Some(source) = source else {
        return Ok(None);
    };
    match (source.month, source.day) {
        (Some(month), Some(day)) => {
            resolve_explicit_date_near(reference, source, month, day).map(Some)
        }
        (None, None) => resolve_clock_near(reference, source).map(Some),
        _ => Err(AdvisoryTimeError::IncompleteDateFields {
            month: source.month,
            day: source.day,
        }),
    }
}

fn resolve_explicit_date_near(
    reference: UtcTime,
    source: FisBTime,
    month: u8,
    day: u8,
) -> Result<ResolvedIssueTime, AdvisoryTimeError> {
    let reference_year = calendar_year(reference)?;
    let mut nearest = None;
    for year in (reference_year - 1)..=(reference_year + 1) {
        let Ok(utc) = utc_from_parts(
            year,
            month,
            day,
            source.hour,
            source.minute,
            source.second.unwrap_or(0),
        ) else {
            continue;
        };
        let distance = utc.as_unix_millis().abs_diff(reference.as_unix_millis());
        if nearest.is_none_or(|(_, _, current)| distance < current) {
            nearest = Some((utc, year, distance));
        }
    }
    nearest
        .map(|(utc, year, _)| ResolvedIssueTime { utc, year, month })
        .ok_or(AdvisoryTimeError::InvalidDate {
            year: reference_year,
            month,
            day,
            hour: source.hour,
            minute: source.minute,
            second: source.second.unwrap_or(0),
        })
}

pub(super) fn utc_for_date_near(
    reference: UtcTime,
    month: u8,
    day: u8,
    hour: u8,
    minute: u8,
) -> Result<UtcTime, AdvisoryTimeError> {
    resolve_explicit_date_near(
        reference,
        FisBTime {
            month: Some(month),
            day: Some(day),
            hour,
            minute,
            second: Some(0),
        },
        month,
        day,
    )
    .map(|resolved| resolved.utc)
}

fn resolve_clock_near(
    reference: UtcTime,
    source: FisBTime,
) -> Result<ResolvedIssueTime, AdvisoryTimeError> {
    let (year, month, day) = calendar_date(reference)?;
    let second = source.second.unwrap_or(0);
    validate_parts(year, month, day, source.hour, source.minute, second)?;
    let day_start = utc_from_parts(year, month, day, 0, 0, 0)?.as_unix_millis();
    let clock_millis =
        (i64::from(source.hour) * 3_600 + i64::from(source.minute) * 60 + i64::from(second))
            * MILLIS_PER_SECOND;
    let mut nearest = None;
    for day_offset in [-1_i64, 0, 1] {
        let utc = UtcTime::from_unix_millis(
            day_start + day_offset * SECONDS_PER_DAY * MILLIS_PER_SECOND + clock_millis,
        );
        let Ok((candidate_year, candidate_month, _)) = calendar_date(utc) else {
            continue;
        };
        let distance = utc.as_unix_millis().abs_diff(reference.as_unix_millis());
        if nearest.is_none_or(|(_, _, _, current)| distance < current) {
            nearest = Some((utc, candidate_year, candidate_month, distance));
        }
    }
    nearest
        .map(|(utc, year, month, _)| ResolvedIssueTime { utc, year, month })
        .ok_or(AdvisoryTimeError::InvalidReferenceTime { value: reference })
}

fn calendar_year(value: UtcTime) -> Result<i32, AdvisoryTimeError> {
    calendar_date(value).map(|(year, _, _)| year)
}

fn calendar_date(value: UtcTime) -> Result<(i32, u8, u8), AdvisoryTimeError> {
    let mut days = value
        .as_unix_millis()
        .div_euclid(MILLIS_PER_SECOND * SECONDS_PER_DAY);
    let mut year = 1970_i32;
    if days < 0 {
        return Err(AdvisoryTimeError::InvalidReferenceTime { value });
    }
    while days >= i64::from(days_in_year(year)) {
        days -= i64::from(days_in_year(year));
        year += 1;
        if year > 2_127 {
            return Err(AdvisoryTimeError::InvalidReferenceTime { value });
        }
    }
    let mut month = 1_u8;
    while days >= i64::from(days_in_month(year, month)) {
        days -= i64::from(days_in_month(year, month));
        month = month.wrapping_add(1);
    }
    let day =
        u8::try_from(days + 1).map_err(|_| AdvisoryTimeError::InvalidReferenceTime { value })?;
    Ok((year, month, day))
}

pub(super) fn utc_from_parts(
    year: i32,
    month: u8,
    day: u8,
    hour: u8,
    minute: u8,
    second: u8,
) -> Result<UtcTime, AdvisoryTimeError> {
    validate_parts(year, month, day, hour, minute, second)?;
    let mut days = 0_i64;
    for current_year in 1970..year {
        days += i64::from(days_in_year(current_year));
    }
    for current_month in 1..month {
        days += i64::from(days_in_month(year, current_month));
    }
    days += i64::from(day) - 1;
    let seconds = days * SECONDS_PER_DAY
        + i64::from(hour) * 3_600
        + i64::from(minute) * 60
        + i64::from(second);
    Ok(UtcTime::from_unix_millis(seconds * MILLIS_PER_SECOND))
}

pub(super) fn utc_for_day_near(
    reference_year: i32,
    reference_month: u8,
    reference: UtcTime,
    day: u8,
    hour: u8,
    minute: u8,
) -> Result<UtcTime, AdvisoryTimeError> {
    let mut nearest = None;
    for offset in [-1_i8, 0, 1] {
        let (year, month) = shifted_month(reference_year, reference_month, offset)?;
        let Ok(candidate) = utc_from_parts(year, month, day, hour, minute, 0) else {
            continue;
        };
        let distance = candidate
            .as_unix_millis()
            .abs_diff(reference.as_unix_millis());
        if nearest.is_none_or(|(_, current_distance)| distance < current_distance) {
            nearest = Some((candidate, distance));
        }
    }
    nearest
        .map(|(candidate, _)| candidate)
        .ok_or(AdvisoryTimeError::InvalidDate {
            year: reference_year,
            month: reference_month,
            day,
            hour,
            minute,
            second: 0,
        })
}

fn shifted_month(year: i32, month: u8, offset: i8) -> Result<(i32, u8), AdvisoryTimeError> {
    if !(1..=12).contains(&month) {
        return Err(AdvisoryTimeError::InvalidMonth { year, month });
    }
    let index = year * 12 + i32::from(month) - 1 + i32::from(offset);
    let shifted_year = index.div_euclid(12);
    let shifted_month = u8::try_from(index.rem_euclid(12) + 1)
        .map_err(|_| AdvisoryTimeError::InvalidMonth { year, month })?;
    Ok((shifted_year, shifted_month))
}

fn validate_parts(
    year: i32,
    month: u8,
    day: u8,
    hour: u8,
    minute: u8,
    second: u8,
) -> Result<(), AdvisoryTimeError> {
    let valid = (1970..=2_127).contains(&year)
        && (1..=12).contains(&month)
        && day > 0
        && day <= days_in_month(year, month)
        && hour < 24
        && minute < 60
        && second < 60;
    if valid {
        Ok(())
    } else {
        Err(AdvisoryTimeError::InvalidDate {
            year,
            month,
            day,
            hour,
            minute,
            second,
        })
    }
}

const fn days_in_year(year: i32) -> u16 {
    if is_leap_year(year) { 366 } else { 365 }
}

const fn days_in_month(year: i32, month: u8) -> u8 {
    match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if is_leap_year(year) => 29,
        2 => 28,
        _ => 0,
    }
}

const fn is_leap_year(year: i32) -> bool {
    year % 4 == 0 && (year % 100 != 0 || year % 400 == 0)
}

#[derive(Clone, Debug, Eq, Error, PartialEq)]
/// Failure to resolve an advisory source date.
pub enum AdvisoryTimeError {
    /// A caller UTC value is outside the supported calendar range.
    #[error("advisory reference time {value:?} is outside the supported range")]
    InvalidReferenceTime {
        /// Rejected caller UTC value.
        value: UtcTime,
    },
    /// A source time contains only one date field.
    #[error("advisory source time has month {month:?} and day {day:?}")]
    IncompleteDateFields {
        /// Supplied month.
        month: Option<u8>,
        /// Supplied day.
        day: Option<u8>,
    },
    /// A complete date has an invalid field or range.
    #[error(
        "advisory date {year:04}-{month:02}-{day:02} {hour:02}:{minute:02}:{second:02} is invalid"
    )]
    InvalidDate {
        /// Supplied year.
        year: i32,
        /// Supplied month.
        month: u8,
        /// Supplied day.
        day: u8,
        /// Supplied hour.
        hour: u8,
        /// Supplied minute.
        minute: u8,
        /// Supplied second.
        second: u8,
    },
    /// A month cannot be used in a calendar operation.
    #[error("advisory month {month} is invalid for year {year}")]
    InvalidMonth {
        /// Supplied year.
        year: i32,
        /// Supplied month.
        month: u8,
    },
}
