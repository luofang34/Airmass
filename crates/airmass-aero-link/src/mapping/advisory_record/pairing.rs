//! Matches Product 12 text and graphical records.

use alloc::vec::Vec;

use super::{append_graphical_group as map_graphical, decoded_metadata, unsupported_format};
use crate::{
    MappingError, WeatherIngressBatch,
    advisory::{advisory_type, graphical_advisory_type, resolve_issue_time},
    mapping::{
        AdvisoryPairing,
        advisory_pairing::{AdvisoryPairKey, PairedAdvisoryType, PendingGraphicalAdvisory},
        copy_bytes, raw_identity_with_record,
    },
};
use aero_link::{
    FisBContributor, FisBExchangeProtocol, FisBProvenance, Product8Record, Product8RecordData,
    Product8ReportStatus,
};

enum Product12TextAction {
    Matched,
    Cached,
    Map,
}

pub(super) fn append_graphical_group(
    output: &mut WeatherIngressBatch,
    protocol: &FisBExchangeProtocol,
    product_id: u16,
    records: &[&Product8Record],
    source: &FisBProvenance,
    pairing: &mut AdvisoryPairing,
) -> Result<(), MappingError> {
    if product_id != 12 {
        let advisory_type = graphical_advisory_type(product_id)
            .ok_or_else(|| unsupported_error(product_id, records))?;
        return map_graphical(
            output,
            protocol,
            product_id,
            records,
            source,
            advisory_type,
            None,
        );
    }
    let Some(first) = records.first() else {
        return unsupported_format(product_id, 8);
    };
    let (metadata, _) = decoded_metadata(8, first, product_id)?;
    if source.contributors.is_empty() {
        return Err(MappingError::AdvisoryPairWithoutContributor);
    }
    for contributor in source.contributors.iter().copied() {
        let key = AdvisoryPairKey::new(
            product_id,
            metadata.report_year,
            metadata.report_number,
            contributor,
        );
        let contributor_source = copy_single_provenance(
            source,
            contributor,
            "graphical advisory contributor provenance",
        )?;
        if let Some(advisory_type) = pairing.advisory_type(key) {
            output.contributors_overflowed |= advisory_type.source.contributors_overflowed;
            map_graphical(
                output,
                protocol,
                product_id,
                records,
                &contributor_source,
                advisory_type.value,
                Some(advisory_type),
            )?;
        } else {
            let pending = copy_pending(
                protocol,
                product_id,
                records,
                &contributor_source,
                contributor,
            )?;
            pairing.hold_graphical(key, pending)?;
        }
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub(super) fn append_record(
    output: &mut WeatherIngressBatch,
    protocol: &FisBExchangeProtocol,
    product_id: u16,
    record_format: u8,
    record: &Product8Record,
    source: &FisBProvenance,
    pairing: &mut AdvisoryPairing,
) -> Result<(), MappingError> {
    if product_id == 12 {
        match match_product_12_text(output, protocol, record_format, record, source, pairing)? {
            Product12TextAction::Matched => return Ok(()),
            Product12TextAction::Cached => {
                return tolerate_unmapped_text(
                    product_id,
                    super::append_record(
                        output,
                        protocol,
                        product_id,
                        record_format,
                        record,
                        source,
                    ),
                );
            }
            Product12TextAction::Map => {}
        }
    }
    let mapped = super::append_record(output, protocol, product_id, record_format, record, source);
    if matches!(product_id, 11 | 15) && is_active_text(record_format, record, product_id)? {
        return tolerate_unmapped_text(product_id, mapped);
    }
    mapped
}

fn match_product_12_text(
    output: &mut WeatherIngressBatch,
    protocol: &FisBExchangeProtocol,
    record_format: u8,
    record: &Product8Record,
    source: &FisBProvenance,
    pairing: &mut AdvisoryPairing,
) -> Result<Product12TextAction, MappingError> {
    let (metadata, text) = decoded_metadata(record_format, record, 12)?;
    match metadata.report_status {
        Some(Product8ReportStatus::Active) => {
            let Some(text) = text else {
                return Ok(Product12TextAction::Map);
            };
            if !text.contains("SIGMET") {
                return Ok(Product12TextAction::Map);
            }
            let advisory_type = advisory_type(12, text.contains("CONVECTIVE SIGMET"))
                .ok_or_else(|| unsupported_error(12, core::slice::from_ref(&record)))?;
            if source.contributors.is_empty() {
                return Err(MappingError::AdvisoryPairWithoutContributor);
            }
            let mut matched_any = false;
            for contributor in source.contributors.iter().copied() {
                let key = AdvisoryPairKey::new(
                    12,
                    metadata.report_year,
                    metadata.report_number,
                    contributor,
                );
                let observation = copy_type_observation(
                    protocol,
                    record,
                    metadata.report_year,
                    source,
                    contributor,
                    advisory_type,
                )?;
                if pairing.match_text(key, observation)? {
                    {
                        let matched = pairing
                            .matched_pair(key)
                            .ok_or(MappingError::AdvisoryPairStateUnavailable)?;
                        append_pending(output, matched.pending, matched.advisory_type)?;
                    }
                    pairing.clear_pending(key);
                    matched_any = true;
                }
            }
            if matched_any {
                return Ok(Product12TextAction::Matched);
            }
            return Ok(Product12TextAction::Cached);
        }
        Some(Product8ReportStatus::Cancelled) => {
            if source.contributors.is_empty() {
                return Err(MappingError::AdvisoryPairWithoutContributor);
            }
            for contributor in source.contributors.iter().copied() {
                pairing.cancel(AdvisoryPairKey::new(
                    12,
                    metadata.report_year,
                    metadata.report_number,
                    contributor,
                ));
            }
        }
        None => {}
    }
    Ok(Product12TextAction::Map)
}

fn append_pending(
    output: &mut WeatherIngressBatch,
    pending: &PendingGraphicalAdvisory,
    advisory_type: &PairedAdvisoryType,
) -> Result<(), MappingError> {
    output.contributors_overflowed |=
        pending.source.contributors_overflowed || advisory_type.source.contributors_overflowed;
    let mut records = Vec::new();
    records
        .try_reserve_exact(pending.records.len())
        .map_err(|source| MappingError::Allocation {
            operation: "pending graphical record references",
            source,
        })?;
    records.extend(pending.records.iter());
    map_graphical(
        output,
        &pending.protocol,
        pending.product_id,
        &records,
        &pending.source,
        advisory_type.value,
        Some(advisory_type),
    )
}

fn copy_type_observation(
    protocol: &FisBExchangeProtocol,
    record: &Product8Record,
    report_year: u8,
    source: &FisBProvenance,
    contributor: FisBContributor,
    value: airmass_core::WeatherAdvisoryType,
) -> Result<PairedAdvisoryType, MappingError> {
    let issue = resolve_issue_time(report_year, protocol.product_time()).map_err(|source| {
        MappingError::Advisory {
            product_id: 12,
            source,
        }
    })?;
    Ok(PairedAdvisoryType {
        value,
        issue_time: issue.map(|time| time.utc),
        product_file_id: protocol.product_file_id(),
        raw_identity: raw_identity_with_record(
            protocol,
            12,
            record.record_index,
            record.record_count,
        ),
        source: copy_single_provenance(source, contributor, "paired advisory type provenance")?,
    })
}

fn copy_pending(
    protocol: &FisBExchangeProtocol,
    product_id: u16,
    records: &[&Product8Record],
    source: &FisBProvenance,
    contributor: FisBContributor,
) -> Result<PendingGraphicalAdvisory, MappingError> {
    let mut copied_records = Vec::new();
    copied_records
        .try_reserve_exact(records.len())
        .map_err(|error| MappingError::Allocation {
            operation: "pending graphical records",
            source: error,
        })?;
    for record in records {
        copied_records.push(copy_graphical_record(record, product_id)?);
    }
    Ok(PendingGraphicalAdvisory {
        protocol: *protocol,
        product_id,
        records: copied_records,
        source: copy_single_provenance(source, contributor, "pending graphical provenance")?,
    })
}

fn copy_single_provenance(
    source: &FisBProvenance,
    contributor: FisBContributor,
    operation: &'static str,
) -> Result<FisBProvenance, MappingError> {
    let mut contributors = Vec::new();
    contributors
        .try_reserve_exact(1)
        .map_err(|error| MappingError::Allocation {
            operation,
            source: error,
        })?;
    contributors.push(contributor);
    Ok(FisBProvenance {
        contributors,
        contributors_overflowed: source.contributors_overflowed,
    })
}

fn is_active_text(
    record_format: u8,
    record: &Product8Record,
    product_id: u16,
) -> Result<bool, MappingError> {
    let (metadata, _) = decoded_metadata(record_format, record, product_id)?;
    Ok(matches!(
        metadata.report_status,
        Some(Product8ReportStatus::Active)
    ))
}

fn tolerate_unmapped_text(
    product_id: u16,
    result: Result<(), MappingError>,
) -> Result<(), MappingError> {
    match result {
        Err(MappingError::Advisory {
            product_id: actual, ..
        }) if actual == product_id => Ok(()),
        other => other,
    }
}

fn copy_graphical_record(
    record: &Product8Record,
    product_id: u16,
) -> Result<Product8Record, MappingError> {
    let data = match &record.data {
        Product8RecordData::Graphical {
            report_number,
            report_year,
        } => Product8RecordData::Graphical {
            report_number: *report_number,
            report_year: *report_year,
        },
        _ => return unsupported_format(product_id, 8),
    };
    Ok(Product8Record {
        record_index: record.record_index,
        record_count: record.record_count,
        category: record.category,
        payload_bytes: copy_bytes(&record.payload_bytes)?,
        data,
    })
}

fn unsupported_error(product_id: u16, records: &[&Product8Record]) -> MappingError {
    let record_format = records
        .first()
        .and_then(|record| match &record.data {
            Product8RecordData::Unsupported { record_format } => Some(*record_format),
            _ => None,
        })
        .unwrap_or(8);
    MappingError::Advisory {
        product_id,
        source: crate::advisory::AdvisoryDecodeError::UnsupportedRecordFormat { record_format },
    }
}
