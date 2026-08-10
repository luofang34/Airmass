//! Maps one graphical advisory contributor.

use aero_link::{FisBContributor, FisBTime};
use airmass_core::{
    RawProductIdentity, SourceProductRevision, TimedField, WeatherAdvisoryType, WeatherIngress,
    WeatherProductId,
};

use super::event_ingress;
use crate::{
    MappingError,
    advisory::{
        AdvisoryDecodeError, AdvisoryRecordInput, decode_graphical_advisory, resolve_issue_time,
    },
    mapping::{advisory_pairing::PairedAdvisoryType, provenance, source_revision, time_evidence},
    text_report::SourceEvidence,
};

#[allow(clippy::too_many_arguments)]
pub(super) fn map_contributor(
    product_id: &WeatherProductId,
    raw_identity: &RawProductIdentity,
    numeric_product_id: u16,
    payload: &[u8],
    product_file_id: Option<u16>,
    product_time: Option<FisBTime>,
    inputs: &[AdvisoryRecordInput<'_>],
    advisory_type: WeatherAdvisoryType,
    paired_type: Option<&PairedAdvisoryType>,
    contributor: FisBContributor,
) -> Result<WeatherIngress, MappingError> {
    let Some(first) = inputs.first() else {
        return Err(MappingError::Advisory {
            product_id: numeric_product_id,
            source: AdvisoryDecodeError::EmptyGraphicalRecords,
        });
    };
    let issue = resolve_issue_time(first.metadata.report_year, product_time).map_err(|source| {
        MappingError::Advisory {
            product_id: numeric_product_id,
            source,
        }
    })?;
    let time = time_evidence(contributor, issue.map(|value| value.utc));
    let provenance = provenance(contributor, raw_identity.clone());
    let evidence =
        SourceEvidence::new(time, airmass_core::QualityEvidence::Unavailable, provenance);
    let graphical_revision = source_revision(contributor, product_file_id);
    let (advisory_type, revision) = type_field(
        advisory_type,
        paired_type,
        contributor,
        graphical_revision,
        &evidence,
    )?;
    let decoded =
        decode_graphical_advisory(inputs, issue, advisory_type, &evidence).map_err(|source| {
            MappingError::Advisory {
                product_id: numeric_product_id,
                source,
            }
        })?;
    event_ingress(
        product_id,
        numeric_product_id,
        revision,
        payload,
        decoded,
        &evidence,
    )
}

fn type_field(
    value: WeatherAdvisoryType,
    paired: Option<&PairedAdvisoryType>,
    graphical_contributor: FisBContributor,
    graphical_revision: SourceProductRevision,
    graphical_evidence: &SourceEvidence,
) -> Result<(TimedField<WeatherAdvisoryType>, SourceProductRevision), MappingError> {
    let Some(paired) = paired else {
        return Ok((graphical_evidence.timed(value), graphical_revision));
    };
    let contributor = paired
        .source
        .contributors
        .iter()
        .find(|candidate| {
            candidate.source_id == graphical_contributor.source_id
                && candidate.source_epoch == graphical_contributor.source_epoch
        })
        .copied()
        .ok_or(MappingError::AdvisoryPairWithoutContributor)?;
    let evidence = SourceEvidence::new(
        time_evidence(contributor, paired.issue_time),
        airmass_core::QualityEvidence::Unavailable,
        provenance(contributor, paired.raw_identity.clone()),
    );
    let paired_revision = source_revision(contributor, paired.product_file_id);
    let revision = SourceProductRevision::new(graphical_revision.get().max(paired_revision.get()));
    Ok((evidence.timed(paired.value), revision))
}
