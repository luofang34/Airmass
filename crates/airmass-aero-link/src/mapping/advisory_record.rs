//! Maps decoded TWGO advisory records to weather ingress.

use alloc::{format, vec::Vec};

use aero_link::{
    FisBContributor, FisBExchangeProduct8, FisBExchangeProtocol, FisBProvenance, FisBTime,
    Product8Record, Product8RecordData, decode_product_8,
};
use airmass_core::{
    AbsenceReason, Cancellation, CancellationReason, FieldState, SourceProductRevision, UtcTime,
    WeatherAdvisoryType, WeatherIngress, WeatherPayload, WeatherProduct, WeatherProductId,
    WeatherValue,
};

use super::{
    MappingError, PAYLOAD_MEDIA_TYPE, copy_bytes, provenance, raw_identity_with_record,
    source_revision, time_evidence,
};
use crate::{
    WeatherIngressBatch,
    advisory::{
        AdvisoryDecodeError, AdvisoryEncoding, AdvisoryRecordInput, AdvisoryRecordMetadata,
        DecodedAdvisoryEvent, advisory_encoding, decode_advisory, one_advisory, product_identity,
        resolve_issue_time,
    },
    mapping::{AdvisoryPairing, advisory_pairing::PairedAdvisoryType},
    text_report::SourceEvidence,
};

mod graphical;
mod legacy;
mod pairing;

pub(super) use legacy::map_file as map_legacy_file;

pub(super) const fn is_twgo_product(product_id: u16) -> bool {
    matches!(advisory_encoding(product_id), Some(AdvisoryEncoding::Twgo))
}

pub(super) const fn is_legacy_product(product_id: u16) -> bool {
    matches!(
        advisory_encoding(product_id),
        Some(AdvisoryEncoding::Text | AdvisoryEncoding::DlacText)
    )
}

pub(super) fn map_file(
    protocol: &FisBExchangeProtocol,
    product_id: u16,
    payload: &[u8],
    source: &FisBProvenance,
    advisory_pairing: &mut AdvisoryPairing,
) -> Result<WeatherIngressBatch, MappingError> {
    let decoded = decode_product_8(payload).map_err(|error| MappingError::AdvisoryEnvelope {
        product_id,
        source: error,
    })?;
    let mut output = WeatherIngressBatch {
        values: Vec::new(),
        contributors_overflowed: source.contributors_overflowed,
    };
    if decoded.record_format == 8 {
        append_graphical_file(
            &mut output,
            protocol,
            product_id,
            &decoded.records,
            source,
            advisory_pairing,
        )?;
    } else {
        for record in &decoded.records {
            pairing::append_record(
                &mut output,
                protocol,
                product_id,
                decoded.record_format,
                record,
                source,
                advisory_pairing,
            )?;
        }
    }
    Ok(output)
}

fn append_graphical_file(
    output: &mut WeatherIngressBatch,
    protocol: &FisBExchangeProtocol,
    product_id: u16,
    records: &[Product8Record],
    source: &FisBProvenance,
    advisory_pairing: &mut AdvisoryPairing,
) -> Result<(), MappingError> {
    let mut processed = Vec::new();
    processed
        .try_reserve_exact(records.len())
        .map_err(|source| MappingError::Allocation {
            operation: "graphical record markers",
            source,
        })?;
    processed.resize(records.len(), false);
    for index in 0..records.len() {
        if processed[index] {
            continue;
        }
        let key = graphical_key(&records[index], product_id)?;
        let mut group = Vec::new();
        for (candidate_index, candidate) in records.iter().enumerate() {
            if !processed[candidate_index] && graphical_key(candidate, product_id)? == key {
                group
                    .try_reserve(1)
                    .map_err(|source| MappingError::Allocation {
                        operation: "graphical report group",
                        source,
                    })?;
                group.push(candidate);
                processed[candidate_index] = true;
            }
        }
        pairing::append_graphical_group(
            output,
            protocol,
            product_id,
            &group,
            source,
            advisory_pairing,
        )?;
    }
    Ok(())
}

fn graphical_key(record: &Product8Record, product_id: u16) -> Result<(u16, u8), MappingError> {
    match &record.data {
        Product8RecordData::Graphical {
            report_number,
            report_year,
        } => Ok((*report_number, *report_year)),
        Product8RecordData::Unsupported { record_format } => {
            unsupported_format(product_id, *record_format)
        }
        _ => unsupported_format(product_id, 8),
    }
}

pub(super) fn append_graphical_group(
    output: &mut WeatherIngressBatch,
    protocol: &FisBExchangeProtocol,
    product_id: u16,
    records: &[&Product8Record],
    source: &FisBProvenance,
    advisory_type: WeatherAdvisoryType,
    paired_type: Option<&PairedAdvisoryType>,
) -> Result<(), MappingError> {
    let inputs = graphical_inputs(records, product_id)?;
    let Some(first) = inputs.first() else {
        return Err(MappingError::Advisory {
            product_id,
            source: AdvisoryDecodeError::EmptyGraphicalRecords,
        });
    };
    let mapped_id = mapped_product_id(product_id, Some(first.metadata));
    let raw_identity = raw_identity_with_record(
        protocol,
        product_id,
        None,
        records.first().and_then(|record| record.record_count),
    );
    let payload = joined_payload(records)?;
    output
        .values
        .try_reserve_exact(source.contributors.len())
        .map_err(|source| MappingError::Allocation {
            operation: "graphical advisory ingress batch",
            source,
        })?;
    for contributor in source.contributors.iter().copied() {
        output.values.push(graphical::map_contributor(
            &mapped_id,
            &raw_identity,
            product_id,
            &payload,
            protocol.product_file_id(),
            protocol.product_time(),
            &inputs,
            advisory_type,
            paired_type,
            contributor,
        )?);
    }
    Ok(())
}

fn graphical_inputs<'a>(
    records: &[&'a Product8Record],
    product_id: u16,
) -> Result<Vec<AdvisoryRecordInput<'a>>, MappingError> {
    let mut inputs = Vec::new();
    inputs
        .try_reserve_exact(records.len())
        .map_err(|source| MappingError::Allocation {
            operation: "graphical advisory inputs",
            source,
        })?;
    for record in records {
        let (metadata, _) = decoded_metadata(8, record, product_id)?;
        inputs.push(AdvisoryRecordInput {
            product_id,
            payload: &record.payload_bytes,
            decoded_text: None,
            metadata,
        });
    }
    Ok(inputs)
}

fn joined_payload(records: &[&Product8Record]) -> Result<Vec<u8>, MappingError> {
    let total = records
        .iter()
        .try_fold(0_usize, |sum, record| {
            sum.checked_add(record.payload_bytes.len())
        })
        .ok_or(MappingError::PayloadLengthOverflow)?;
    let mut payload = Vec::new();
    payload
        .try_reserve_exact(total)
        .map_err(|source| MappingError::Allocation {
            operation: "joined graphical payload",
            source,
        })?;
    for record in records {
        payload.extend_from_slice(&record.payload_bytes);
    }
    Ok(payload)
}

pub(super) fn append_record(
    output: &mut WeatherIngressBatch,
    protocol: &FisBExchangeProtocol,
    product_id: u16,
    record_format: u8,
    record: &Product8Record,
    source: &FisBProvenance,
) -> Result<(), MappingError> {
    let (metadata, text) = decoded_metadata(record_format, record, product_id)?;
    let mapped_id = mapped_product_id(product_id, Some(metadata));
    let raw_identity = raw_identity_with_record(
        protocol,
        product_id,
        record.record_index,
        record.record_count,
    );
    output
        .values
        .try_reserve_exact(source.contributors.len())
        .map_err(|error| MappingError::Allocation {
            operation: "advisory ingress batch",
            source: error,
        })?;
    for contributor in source.contributors.iter().copied() {
        output.values.push(map_contributor(
            &mapped_id,
            &raw_identity,
            product_id,
            &record.payload_bytes,
            protocol.product_file_id(),
            protocol.product_time(),
            text,
            metadata,
            contributor,
        )?);
    }
    Ok(())
}

pub(super) fn decoded_metadata(
    record_format: u8,
    record: &Product8Record,
    product_id: u16,
) -> Result<(AdvisoryRecordMetadata, Option<&str>), MappingError> {
    let (report_number, report_year, report_status, text) = match &record.data {
        Product8RecordData::Text {
            report_number,
            report_year,
            report_status,
            text,
        } => (
            *report_number,
            *report_year,
            Some(*report_status),
            Some(text.as_str()),
        ),
        Product8RecordData::Graphical {
            report_number,
            report_year,
        } => (*report_number, *report_year, None, None),
        Product8RecordData::Unsupported { record_format } => {
            return unsupported_format(product_id, *record_format);
        }
        _ => return unsupported_format(product_id, record_format),
    };
    Ok((
        AdvisoryRecordMetadata {
            record_format,
            report_number,
            report_year,
            report_status,
        },
        text,
    ))
}

fn unsupported_format<T>(product_id: u16, record_format: u8) -> Result<T, MappingError> {
    Err(MappingError::Advisory {
        product_id,
        source: AdvisoryDecodeError::UnsupportedRecordFormat { record_format },
    })
}

pub(super) fn exchange_metadata(value: &FisBExchangeProduct8) -> AdvisoryRecordMetadata {
    AdvisoryRecordMetadata {
        record_format: value.record_format(),
        report_number: value.report_number(),
        report_year: value.report_year(),
        report_status: value.report_status(),
    }
}

pub(super) fn mapped_product_id(
    numeric_product_id: u16,
    metadata: Option<AdvisoryRecordMetadata>,
) -> WeatherProductId {
    metadata.map_or_else(
        || WeatherProductId::new(format!("fisb-product-{numeric_product_id}")),
        |value| WeatherProductId::new(product_identity(numeric_product_id, value)),
    )
}

#[allow(clippy::too_many_arguments)]
pub(super) fn map_contributor(
    product_id: &WeatherProductId,
    raw_identity: &airmass_core::RawProductIdentity,
    numeric_product_id: u16,
    payload: &[u8],
    product_file_id: Option<u16>,
    product_time: Option<FisBTime>,
    decoded_text: Option<&str>,
    metadata: AdvisoryRecordMetadata,
    contributor: FisBContributor,
) -> Result<WeatherIngress, MappingError> {
    let issue = resolve_issue_time(metadata.report_year, product_time).map_err(|source| {
        MappingError::Advisory {
            product_id: numeric_product_id,
            source,
        }
    })?;
    let time = time_evidence(contributor, issue.map(|value| value.utc));
    let quality = airmass_core::QualityEvidence::Unavailable;
    let provenance = provenance(contributor, raw_identity.clone());
    let evidence = SourceEvidence::new(time, quality, provenance);
    let decoded = decode_advisory(
        AdvisoryRecordInput {
            product_id: numeric_product_id,
            payload,
            decoded_text,
            metadata,
        },
        issue,
        &evidence,
    )
    .map_err(|source| MappingError::Advisory {
        product_id: numeric_product_id,
        source,
    })?;
    event_ingress(
        product_id,
        numeric_product_id,
        source_revision(contributor, product_file_id),
        payload,
        decoded,
        &evidence,
    )
}

fn event_ingress(
    product_id: &WeatherProductId,
    numeric_product_id: u16,
    revision: SourceProductRevision,
    payload: &[u8],
    decoded: DecodedAdvisoryEvent,
    evidence: &SourceEvidence,
) -> Result<WeatherIngress, MappingError> {
    match decoded {
        DecodedAdvisoryEvent::Product {
            advisory,
            issue_time,
            valid_period,
        } => product_ingress(
            product_id,
            numeric_product_id,
            revision,
            payload,
            advisory,
            issue_time,
            valid_period,
            evidence,
        ),
        DecodedAdvisoryEvent::Cancellation { issue_time } => Ok(WeatherIngress::cancellation(
            product_id.clone(),
            revision,
            Cancellation::new(
                evidence.timed(CancellationReason::Withdrawn),
                optional_time(issue_time, evidence),
            ),
        )),
    }
}

#[allow(clippy::too_many_arguments)]
fn product_ingress(
    product_id: &WeatherProductId,
    numeric_product_id: u16,
    revision: SourceProductRevision,
    payload_bytes: &[u8],
    advisory: airmass_core::WeatherAdvisory,
    issue_time: Option<UtcTime>,
    valid_period: airmass_core::UtcInterval,
    evidence: &SourceEvidence,
) -> Result<WeatherIngress, MappingError> {
    let payload = evidence.timed(WeatherPayload::new(
        PAYLOAD_MEDIA_TYPE,
        copy_bytes(payload_bytes)?,
    ));
    let values = one_advisory(advisory).map_err(|source| MappingError::Advisory {
        product_id: numeric_product_id,
        source,
    })?;
    let product = WeatherProduct::new(
        payload,
        evidence.present(WeatherValue::Advisories(values)),
        optional_time(issue_time, evidence),
        evidence.present(valid_period.start),
        evidence.present(valid_period),
        evidence.absent(AbsenceReason::NotObserved),
    );
    Ok(WeatherIngress::product(
        product_id.clone(),
        revision,
        product,
    ))
}

fn optional_time(value: Option<UtcTime>, evidence: &SourceEvidence) -> FieldState<UtcTime> {
    value.map_or_else(
        || evidence.absent(AbsenceReason::NotObserved),
        |time| evidence.present(time),
    )
}
