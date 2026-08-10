//! Maps decoded DLAC reports to source-neutral weather reports.

use aero_link::{FisBDecodedProduct, TextReport, decode_fis_b_product};
use airmass_core::{
    AbsenceEvidence, AbsenceReason, AbsentField, FieldState, FlightCategory, ProductProvenance,
    QualityEvidence, TextWeatherReport, TimeEvidence, TimedField, WeatherReportTime,
    WeatherReportType, WeatherStationId, WeatherValue,
};

mod metar;

#[cfg(test)]
mod tests;

const TEXT_PRODUCT_ID: u16 = 413;

#[derive(Clone, Debug)]
pub(crate) struct SourceEvidence {
    time: TimeEvidence,
    quality: QualityEvidence,
    provenance: ProductProvenance,
}

impl SourceEvidence {
    pub(crate) const fn new(
        time: TimeEvidence,
        quality: QualityEvidence,
        provenance: ProductProvenance,
    ) -> Self {
        Self {
            time,
            quality,
            provenance,
        }
    }

    pub(crate) fn timed<T>(&self, value: T) -> TimedField<T> {
        TimedField::new(
            value,
            self.time,
            self.quality.clone(),
            self.provenance.clone(),
        )
    }

    pub(crate) fn present<T>(&self, value: T) -> FieldState<T> {
        FieldState::present(self.timed(value))
    }

    pub(crate) fn absent<T>(&self, reason: AbsenceReason) -> FieldState<T> {
        FieldState::absent(AbsentField::new(
            reason,
            AbsenceEvidence::Source {
                time: self.time,
                quality: self.quality.clone(),
                provenance: self.provenance.clone(),
            },
        ))
    }
}

pub(crate) fn decode_weather_value(
    product_id: u16,
    payload: &[u8],
    evidence: &SourceEvidence,
) -> FieldState<WeatherValue> {
    if product_id != TEXT_PRODUCT_ID {
        return evidence.absent(AbsenceReason::Unsupported);
    }
    let FisBDecodedProduct::GenericText { reports } = decode_fis_b_product(product_id, payload)
    else {
        return evidence.absent(AbsenceReason::Unsupported);
    };
    if reports.is_empty() {
        return evidence.absent(AbsenceReason::NotObserved);
    }
    let reports = reports
        .into_iter()
        .map(|report| map_report(report, evidence))
        .collect();
    evidence.present(WeatherValue::TextReports(reports))
}

fn map_report(report: TextReport, evidence: &SourceEvidence) -> TextWeatherReport {
    let layout = report_layout(&report);
    let report_type =
        mapped_report_type(report.report_type.as_deref(), layout.report_type, evidence);
    let station_id = layout.station_id.map_or_else(
        || evidence.absent(AbsenceReason::NotObserved),
        |value| evidence.present(WeatherStationId::new(value)),
    );
    let report_time = mapped_report_time(layout.report_time, evidence);
    let flight_category = mapped_flight_category(
        report.report_type.as_deref(),
        layout.report_type,
        &report.text,
        evidence,
    );
    TextWeatherReport::new(
        report_type,
        station_id,
        report_time,
        evidence.timed(report.text),
        flight_category,
    )
}

#[derive(Clone, Copy, Debug)]
struct ReportLayout<'a> {
    report_type: Option<WeatherReportType>,
    station_id: Option<&'a str>,
    report_time: Option<&'a str>,
}

fn report_layout(report: &TextReport) -> ReportLayout<'_> {
    if report.report_type.as_deref() == Some("TAF") && report.location.as_deref() == Some("AMD") {
        return ReportLayout {
            report_type: Some(WeatherReportType::AmendedTaf),
            station_id: report.issue_time.as_deref(),
            report_time: report.text.split_ascii_whitespace().next(),
        };
    }
    ReportLayout {
        report_type: report_type(report.report_type.as_deref()),
        station_id: report.location.as_deref(),
        report_time: report.issue_time.as_deref(),
    }
}

fn mapped_report_type(
    raw: Option<&str>,
    recognized: Option<WeatherReportType>,
    evidence: &SourceEvidence,
) -> FieldState<WeatherReportType> {
    match (raw, recognized) {
        (_, Some(value)) => evidence.present(value),
        (Some(_), None) => evidence.absent(AbsenceReason::Unsupported),
        (None, None) => evidence.absent(AbsenceReason::NotObserved),
    }
}

fn mapped_report_time(
    raw: Option<&str>,
    evidence: &SourceEvidence,
) -> FieldState<WeatherReportTime> {
    match raw {
        Some(value) => parse_report_time(value).map_or_else(
            || evidence.absent(AbsenceReason::Rejected),
            |time| evidence.present(time),
        ),
        None => evidence.absent(AbsenceReason::NotObserved),
    }
}

fn mapped_flight_category(
    raw_type: Option<&str>,
    report_type: Option<WeatherReportType>,
    text: &str,
    evidence: &SourceEvidence,
) -> FieldState<FlightCategory> {
    match report_type {
        Some(WeatherReportType::Metar | WeatherReportType::Speci) => metar::flight_category(text)
            .map_or_else(
                || evidence.absent(AbsenceReason::NotObserved),
                |category| evidence.present(category),
            ),
        Some(_) => evidence.absent(AbsenceReason::NotApplicable),
        None if raw_type.is_some() => evidence.absent(AbsenceReason::Unsupported),
        None => evidence.absent(AbsenceReason::NotObserved),
    }
}

fn report_type(value: Option<&str>) -> Option<WeatherReportType> {
    match value {
        Some("METAR") => Some(WeatherReportType::Metar),
        Some("SPECI") => Some(WeatherReportType::Speci),
        Some("TAF") => Some(WeatherReportType::Taf),
        Some("PIREP") => Some(WeatherReportType::Pirep),
        Some("WINDS") => Some(WeatherReportType::WindsAndTemperaturesAloft),
        Some(_) | None => None,
    }
}

fn parse_report_time(value: &str) -> Option<WeatherReportTime> {
    if value.len() != 7 || !value.ends_with('Z') {
        return None;
    }
    let day = value.get(0..2)?.parse().ok()?;
    let hour = value.get(2..4)?.parse().ok()?;
    let minute = value.get(4..6)?.parse().ok()?;
    WeatherReportTime::new(day, hour, minute)
}
