//! Derives a flight category from METAR observation groups.

use airmass_core::{Ceiling, FlightCategory, StatuteMiles};

pub(super) fn flight_category(text: &str) -> Option<FlightCategory> {
    let mut visibility = None;
    let mut ceiling_height = None;
    let mut sky_observed = false;
    let mut ceiling_indeterminate = false;
    let mut previous = None;
    for token in text.split_ascii_whitespace() {
        if token == "RMK" {
            break;
        }
        if visibility.is_none() {
            visibility = parse_visibility(token, previous);
        }
        if is_clear_sky(token) {
            sky_observed = true;
        }
        if let Some(cover) = sky_cover(token) {
            sky_observed = true;
            if matches!(cover, "BKN" | "OVC" | "VV") {
                match parse_ceiling(token) {
                    Some(value) => {
                        ceiling_height =
                            Some(ceiling_height.map_or(value, |current: u32| current.min(value)));
                    }
                    None => ceiling_indeterminate = true,
                }
            }
        }
        previous = Some(token);
    }
    let ceiling = ceiling(ceiling_height, sky_observed, ceiling_indeterminate)?;
    Some(FlightCategory::from_conditions(ceiling, visibility?))
}

fn ceiling(height: Option<u32>, sky_observed: bool, indeterminate: bool) -> Option<Ceiling> {
    if indeterminate {
        None
    } else if let Some(feet) = height {
        Some(Ceiling::Height(feet))
    } else if sky_observed {
        Some(Ceiling::NoCeiling)
    } else {
        None
    }
}

fn parse_visibility(token: &str, previous: Option<&str>) -> Option<StatuteMiles> {
    let body = token.strip_suffix("SM")?;
    let body = body
        .strip_prefix('M')
        .or_else(|| body.strip_prefix('P'))
        .unwrap_or(body);
    let (mut numerator, denominator) = parse_number(body)?;
    if body.contains('/')
        && let Some(whole) = previous.and_then(parse_whole)
    {
        numerator = whole.checked_mul(denominator)?.checked_add(numerator)?;
    }
    StatuteMiles::new(numerator, denominator)
}

fn parse_number(value: &str) -> Option<(u32, u32)> {
    match value.split_once('/') {
        Some((numerator, denominator)) => {
            let numerator = numerator.parse().ok()?;
            let denominator = denominator.parse().ok()?;
            (denominator != 0).then_some((numerator, denominator))
        }
        None => Some((value.parse().ok()?, 1)),
    }
}

fn parse_whole(value: &str) -> Option<u32> {
    value
        .bytes()
        .all(|byte| byte.is_ascii_digit())
        .then(|| value.parse().ok())
        .flatten()
}

fn parse_ceiling(token: &str) -> Option<u32> {
    let height = if token.starts_with("VV") {
        token.get(2..5)?
    } else {
        token.get(3..6)?
    };
    height.parse::<u32>().ok()?.checked_mul(100)
}

fn sky_cover(token: &str) -> Option<&str> {
    if token.starts_with("VV") {
        return Some("VV");
    }
    token
        .get(0..3)
        .filter(|cover| matches!(*cover, "FEW" | "SCT" | "BKN" | "OVC"))
}

fn is_clear_sky(token: &str) -> bool {
    matches!(token, "CLR" | "SKC" | "NSC" | "NCD")
}
