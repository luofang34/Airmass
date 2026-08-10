//! Maps classified protocol exchange records to source-neutral weather data.

use alloc::{collections::TryReserveError, format, string::String, vec::Vec};

use aero_link::{
    FisBCategory, FisBContributor, FisBDeliveryPath, FisBExchangeError, FisBExchangeProtocol,
    FisBExchangeRecord, FisBOrigin, FisBSourceTimeQuality, FisBTime, Product8DecodeError,
};
use airmass_core::{
    AbsenceEvidence, AbsenceReason, AbsentField, DeliveryPath, FieldState, IngressTimeBasis,
    MonotonicTime, ProductProvenance, QualityEvidence, RawProductIdentity, SourceEpoch, SourceId,
    SourceProductRevision, SourceRef, SourceTime, SourceTimeKind, TimeBasis, TimeEvidence,
    TimeQuality, TimeUncertainty, TimedField, UtcTime, WeatherIngress, WeatherOrigin,
    WeatherPayload, WeatherProduct, WeatherProductId,
};
use thiserror::Error;

use crate::{
    WeatherIngressBatch,
    advisory::{AdvisoryDecodeError, AdvisoryRecordMetadata, is_advisory_product},
    text_report::{SourceEvidence, decode_weather_value},
};

mod advisory_pairing;
mod advisory_record;

pub(crate) use advisory_pairing::AdvisoryPairing;

const PAYLOAD_MEDIA_TYPE: &str = "application/vnd.aero-link.fis-b";
const RAW_ID_NAMESPACE: &str = "aero-link/fis-b-exchange-v1";

pub(crate) fn map_records(
    records: Vec<FisBExchangeRecord>,
    received_utc: Option<UtcTime>,
    advisory_pairing: &mut AdvisoryPairing,
) -> Result<WeatherIngressBatch, MappingError> {
    let mut output = WeatherIngressBatch::default();
    for record in records {
        if !is_weather_record(&record)? {
            continue;
        }
        let mut mapped = map_weather_record(&record, received_utc, advisory_pairing)?;
        output.contributors_overflowed |= mapped.contributors_overflowed;
        output
            .values
            .try_reserve_exact(mapped.values.len())
            .map_err(|source| MappingError::Allocation {
                operation: "weather ingress output",
                source,
            })?;
        output.values.append(&mut mapped.values);
    }
    Ok(output)
}

fn is_weather_record(record: &FisBExchangeRecord) -> Result<bool, MappingError> {
    if record.category()? == FisBCategory::Weather {
        return Ok(true);
    }
    Ok(record
        .protocol()?
        .product_id()
        .is_some_and(is_advisory_product))
}

fn map_weather_record(
    record: &FisBExchangeRecord,
    received_utc: Option<UtcTime>,
    advisory_pairing: &mut AdvisoryPairing,
) -> Result<WeatherIngressBatch, MappingError> {
    let protocol = record.protocol()?;
    let Some(numeric_product_id) = protocol.product_id() else {
        return Err(MappingError::WeatherRecordWithoutProductId);
    };
    let source_provenance = record.provenance()?;
    let payload = record.payload_bytes()?;
    if advisory_record::is_legacy_product(numeric_product_id) {
        return advisory_record::map_legacy_file(
            protocol,
            numeric_product_id,
            payload,
            source_provenance,
            received_utc,
        );
    }
    if advisory_record::is_twgo_product(numeric_product_id) && record.product_8()?.is_none() {
        return advisory_record::map_file(
            protocol,
            numeric_product_id,
            payload,
            source_provenance,
            advisory_pairing,
        );
    }
    let raw_identity = raw_identity(protocol, numeric_product_id);
    let advisory_metadata = if is_advisory_product(numeric_product_id) {
        record.product_8()?.map(advisory_record::exchange_metadata)
    } else {
        None
    };
    let product_id = advisory_record::mapped_product_id(numeric_product_id, advisory_metadata);
    let decoded_text = record.decoded_text()?;
    let mut values = Vec::new();
    values
        .try_reserve_exact(source_provenance.contributors.len())
        .map_err(|source| MappingError::Allocation {
            operation: "weather ingress batch",
            source,
        })?;
    for contributor in source_provenance.contributors.iter().copied() {
        values.push(map_contributor(
            &product_id,
            &raw_identity,
            numeric_product_id,
            payload,
            protocol.product_file_id(),
            protocol.product_time(),
            decoded_text,
            advisory_metadata,
            contributor,
        )?);
    }
    Ok(WeatherIngressBatch {
        values,
        contributors_overflowed: source_provenance.contributors_overflowed,
    })
}

#[allow(
    clippy::too_many_arguments,
    reason = "the protocol record and contributor supply separate evidence"
)]
fn map_contributor(
    product_id: &WeatherProductId,
    raw_identity: &RawProductIdentity,
    numeric_product_id: u16,
    payload: &[u8],
    product_file_id: Option<u16>,
    product_time: Option<FisBTime>,
    decoded_text: Option<&str>,
    advisory_metadata: Option<AdvisoryRecordMetadata>,
    contributor: FisBContributor,
) -> Result<WeatherIngress, MappingError> {
    if let Some(metadata) = advisory_metadata {
        return advisory_record::map_contributor(
            product_id,
            raw_identity,
            numeric_product_id,
            payload,
            product_file_id,
            product_time,
            decoded_text,
            metadata,
            contributor,
        );
    }
    let provenance = provenance(contributor, raw_identity.clone());
    let time = time_evidence(contributor, None);
    let quality = QualityEvidence::Unavailable;
    let evidence = SourceEvidence::new(time, quality.clone(), provenance.clone());
    let typed_value = decode_weather_value(numeric_product_id, payload, &evidence);
    let payload = TimedField::new(
        WeatherPayload::new(PAYLOAD_MEDIA_TYPE, copy_bytes(payload)?),
        time,
        quality.clone(),
        provenance.clone(),
    );
    let source_absence = |reason| source_absence(reason, time, quality.clone(), provenance.clone());
    let product = WeatherProduct::new(
        payload,
        typed_value,
        FieldState::absent(source_absence(AbsenceReason::Unsupported)),
        FieldState::absent(source_absence(AbsenceReason::NotObserved)),
        FieldState::absent(source_absence(AbsenceReason::NotObserved)),
        FieldState::absent(source_absence(AbsenceReason::NotObserved)),
    );
    Ok(WeatherIngress::product(
        product_id.clone(),
        source_revision(contributor, product_file_id),
        product,
    ))
}

fn source_absence(
    reason: AbsenceReason,
    time: TimeEvidence,
    quality: QualityEvidence,
    provenance: ProductProvenance,
) -> AbsentField {
    AbsentField::new(
        reason,
        AbsenceEvidence::Source {
            time,
            quality,
            provenance,
        },
    )
}

fn provenance(contributor: FisBContributor, raw_identity: RawProductIdentity) -> ProductProvenance {
    ProductProvenance::new(
        SourceRef::new(
            SourceId::new(format!("aero-link-source-{}", contributor.source_id)),
            SourceEpoch::new(u64::from(contributor.source_epoch)),
        ),
        origin(contributor.origin),
        DeliveryPath::new(delivery_path(contributor.delivery_path)),
        Some(raw_identity),
    )
}

fn time_evidence(contributor: FisBContributor, issue_time: Option<UtcTime>) -> TimeEvidence {
    TimeEvidence::with_ingress_evidence(
        MonotonicTime::from_micros(contributor.last_received_at_micros),
        ingress_basis(contributor.source_time_quality),
        contributor
            .uncertainty_micros
            .map_or(TimeUncertainty::Unknown, TimeUncertainty::Bounded),
        issue_time.map_or(SourceTime::Unknown, |value| SourceTime::Known {
            value,
            kind: SourceTimeKind::Issue,
            basis: TimeBasis::AdapterResolved,
            quality: TimeQuality::Coarse,
            uncertainty: TimeUncertainty::Unknown,
        }),
    )
}

fn source_revision(
    contributor: FisBContributor,
    product_file_id: Option<u16>,
) -> SourceProductRevision {
    let file_id = u64::from(product_file_id.unwrap_or(0));
    SourceProductRevision::new((contributor.last_received_at_micros << 10) | file_id)
}

fn raw_identity(protocol: &FisBExchangeProtocol, product_id: u16) -> RawProductIdentity {
    raw_identity_with_record(
        protocol,
        product_id,
        protocol.record_index(),
        protocol.record_count(),
    )
}

fn raw_identity_with_record(
    protocol: &FisBExchangeProtocol,
    product_id: u16,
    record_index: Option<u8>,
    record_count: Option<u8>,
) -> RawProductIdentity {
    RawProductIdentity::new(
        RAW_ID_NAMESPACE,
        format!(
            "product={product_id};file={:?};segments={};record={:?}/{:?};time={}",
            protocol.product_file_id(),
            protocol.segment_count(),
            record_index,
            record_count,
            protocol
                .product_time()
                .map_or_else(|| String::from("unknown"), product_time)
        ),
    )
}

fn product_time(time: FisBTime) -> String {
    format!(
        "{:?}-{:?}T{:02}:{:02}:{:?}",
        time.month, time.day, time.hour, time.minute, time.second
    )
}

fn copy_bytes(bytes: &[u8]) -> Result<Vec<u8>, MappingError> {
    let mut copy = Vec::new();
    copy.try_reserve_exact(bytes.len())
        .map_err(|source| MappingError::Allocation {
            operation: "weather payload",
            source,
        })?;
    copy.extend_from_slice(bytes);
    Ok(copy)
}

fn origin(value: FisBOrigin) -> WeatherOrigin {
    match value {
        FisBOrigin::Receiver => WeatherOrigin::LocalReceiver,
        FisBOrigin::Network => WeatherOrigin::NetworkProvider,
        FisBOrigin::Replay => WeatherOrigin::Replay,
        FisBOrigin::Simulator => WeatherOrigin::Simulation,
        FisBOrigin::UatGroundStation => WeatherOrigin::Other("uat-ground-station".into()),
        FisBOrigin::Unknown => WeatherOrigin::Other("unknown".into()),
        _ => WeatherOrigin::Other("future-aero-link-origin".into()),
    }
}

fn delivery_path(value: FisBDeliveryPath) -> &'static str {
    match value {
        FisBDeliveryPath::Unspecified => "unspecified",
        FisBDeliveryPath::LocalReceiver => "local-receiver",
        FisBDeliveryPath::InstalledAvionics => "installed-avionics",
        FisBDeliveryPath::NetworkProvider => "network-provider",
        FisBDeliveryPath::Replay => "replay",
        FisBDeliveryPath::Simulator => "simulator",
        _ => "future-aero-link-delivery-path",
    }
}

fn ingress_basis(value: FisBSourceTimeQuality) -> IngressTimeBasis {
    match value {
        FisBSourceTimeQuality::LocalIngress => IngressTimeBasis::LocalIngress,
        FisBSourceTimeQuality::MappedSourceTime => IngressTimeBasis::MappedSourceTime,
        FisBSourceTimeQuality::UnknownSourceTime => IngressTimeBasis::Unknown,
        _ => IngressTimeBasis::Unknown,
    }
}

/// Failure while mapping classified protocol data.
#[non_exhaustive]
#[derive(Debug, Error)]
pub enum MappingError {
    /// A protocol exchange record is invalid.
    #[error("invalid FIS-B exchange record")]
    Exchange(#[from] FisBExchangeError),
    /// A weather record has no protocol product identifier.
    #[error("FIS-B weather record has no product identifier")]
    WeatherRecordWithoutProductId,
    /// Advisory pairing provenance has no contributor.
    #[error("FIS-B advisory pairing provenance has no contributor")]
    AdvisoryPairWithoutContributor,
    /// Advisory pairing state lost a stored type.
    #[error("FIS-B advisory pairing state lost a stored type")]
    AdvisoryPairStateUnavailable,
    /// A TWGO advisory file could not be decoded.
    #[error("could not decode FIS-B TWGO advisory product {product_id}")]
    AdvisoryEnvelope {
        /// Numeric FIS-B product identifier.
        product_id: u16,
        /// TWGO decoding failure.
        #[source]
        source: Product8DecodeError,
    },
    /// A weather advisory record could not be decoded.
    #[error("could not decode FIS-B weather advisory product {product_id}")]
    Advisory {
        /// Numeric FIS-B product identifier.
        product_id: u16,
        /// Typed advisory decoding failure.
        #[source]
        source: AdvisoryDecodeError,
    },
    /// A bounded output allocation failed.
    #[error("allocation failed for {operation}")]
    Allocation {
        /// Output being allocated.
        operation: &'static str,
        /// Allocation failure.
        #[source]
        source: TryReserveError,
    },
    /// Combined source payload length does not fit in memory indexing.
    #[error("combined FIS-B advisory payload length overflowed")]
    PayloadLengthOverflow,
}
