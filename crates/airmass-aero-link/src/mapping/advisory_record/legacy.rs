//! Maps legacy text advisories to weather ingress.

use alloc::vec::Vec;

use aero_link::{FisBContributor, FisBExchangeProtocol, FisBProvenance};
use airmass_core::{QualityEvidence, UtcTime, WeatherIngress, WeatherProductId};

use super::{event_ingress, raw_identity_with_record, source_revision, time_evidence};
use crate::{
    MappingError, WeatherIngressBatch,
    advisory::{
        AdvisoryDecodeError, decode_legacy_advisory, decode_legacy_payload,
        legacy_product_identity, resolve_legacy_issue_time,
    },
    mapping::provenance,
    text_report::SourceEvidence,
};

pub(in crate::mapping) fn map_file(
    protocol: &FisBExchangeProtocol,
    product_id: u16,
    payload: &[u8],
    source: &FisBProvenance,
    received_utc: Option<UtcTime>,
) -> Result<WeatherIngressBatch, MappingError> {
    let reference = received_utc
        .ok_or_else(|| advisory_error(product_id, AdvisoryDecodeError::MissingCivilUtc))?;
    let text = decode_legacy_payload(product_id, payload)
        .map_err(|source| advisory_error(product_id, source))?;
    let identity = legacy_product_identity(product_id, &text)
        .map_err(|source| advisory_error(product_id, source))?;
    let issue = resolve_legacy_issue_time(reference, protocol.product_time())
        .map_err(|source| advisory_error(product_id, source))?;
    let mapped_id = WeatherProductId::new(identity.clone());
    let raw_identity = raw_identity_with_record(protocol, product_id, None, None);
    let mut values = Vec::new();
    values
        .try_reserve_exact(source.contributors.len())
        .map_err(|source| MappingError::Allocation {
            operation: "legacy advisory ingress batch",
            source,
        })?;
    for contributor in source.contributors.iter().copied() {
        values.push(map_contributor(
            &mapped_id,
            &raw_identity,
            product_id,
            payload,
            &text,
            identity.clone(),
            issue,
            protocol.product_file_id(),
            contributor,
        )?);
    }
    Ok(WeatherIngressBatch {
        values,
        contributors_overflowed: source.contributors_overflowed,
    })
}

#[allow(clippy::too_many_arguments)]
fn map_contributor(
    product_id: &WeatherProductId,
    raw_identity: &airmass_core::RawProductIdentity,
    numeric_product_id: u16,
    payload: &[u8],
    text: &str,
    identity: alloc::string::String,
    issue: Option<crate::advisory::ResolvedIssueTime>,
    product_file_id: Option<u16>,
    contributor: FisBContributor,
) -> Result<WeatherIngress, MappingError> {
    let time = time_evidence(contributor, issue.map(|value| value.utc));
    let provenance = provenance(contributor, raw_identity.clone());
    let evidence = SourceEvidence::new(time, QualityEvidence::Unavailable, provenance);
    let decoded = decode_legacy_advisory(numeric_product_id, text, identity, issue, &evidence)
        .map_err(|source| advisory_error(numeric_product_id, source))?;
    event_ingress(
        product_id,
        numeric_product_id,
        source_revision(contributor, product_file_id),
        payload,
        decoded,
        &evidence,
    )
}

fn advisory_error(product_id: u16, source: AdvisoryDecodeError) -> MappingError {
    MappingError::Advisory { product_id, source }
}
