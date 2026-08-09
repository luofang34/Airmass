//! Maps classified protocol exchange records to source-neutral weather data.

use alloc::{collections::TryReserveError, format, string::String, vec::Vec};

use aero_link::{
    FisBCategory, FisBContributor, FisBDeliveryPath, FisBExchangeError, FisBExchangeProtocol,
    FisBExchangeRecord, FisBOrigin, FisBSourceTimeQuality, FisBTime,
};
use airmass_core::{
    AbsenceEvidence, AbsenceReason, AbsentField, DeliveryPath, FieldState, IngressTimeBasis,
    MonotonicTime, ProductProvenance, QualityEvidence, RawProductIdentity, SourceEpoch, SourceId,
    SourceProductRevision, SourceRef, SourceTime, TimeEvidence, TimeUncertainty, TimedField,
    WeatherIngress, WeatherOrigin, WeatherPayload, WeatherProduct, WeatherProductId,
};
use thiserror::Error;

use crate::WeatherIngressBatch;

const PAYLOAD_MEDIA_TYPE: &str = "application/vnd.aero-link.fis-b";
const RAW_ID_NAMESPACE: &str = "aero-link/fis-b-exchange-v1";

pub(crate) fn map_records(
    records: Vec<FisBExchangeRecord>,
) -> Result<WeatherIngressBatch, MappingError> {
    let mut output = WeatherIngressBatch::default();
    for record in records {
        if record.category()? != FisBCategory::Weather {
            continue;
        }
        let mut mapped = map_weather_record(&record)?;
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

fn map_weather_record(record: &FisBExchangeRecord) -> Result<WeatherIngressBatch, MappingError> {
    let protocol = record.protocol()?;
    let Some(numeric_product_id) = protocol.product_id() else {
        return Err(MappingError::WeatherRecordWithoutProductId);
    };
    let source_provenance = record.provenance()?;
    let payload = record.payload_bytes()?;
    let raw_identity = raw_identity(protocol, numeric_product_id);
    let product_id = WeatherProductId::new(format!("fisb-product-{numeric_product_id}"));
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
            payload,
            protocol.product_file_id(),
            contributor,
        )?);
    }
    Ok(WeatherIngressBatch {
        values,
        contributors_overflowed: source_provenance.contributors_overflowed,
    })
}

fn map_contributor(
    product_id: &WeatherProductId,
    raw_identity: &RawProductIdentity,
    payload: &[u8],
    product_file_id: Option<u16>,
    contributor: FisBContributor,
) -> Result<WeatherIngress, MappingError> {
    let provenance = provenance(contributor, raw_identity.clone());
    let time = time_evidence(contributor);
    let quality = QualityEvidence::Unavailable;
    let payload = TimedField::new(
        WeatherPayload::new(PAYLOAD_MEDIA_TYPE, copy_bytes(payload)?),
        time,
        quality.clone(),
        provenance.clone(),
    );
    let source_absence = |reason| source_absence(reason, time, quality.clone(), provenance.clone());
    let product = WeatherProduct::new(
        payload,
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

fn time_evidence(contributor: FisBContributor) -> TimeEvidence {
    TimeEvidence::with_ingress_evidence(
        MonotonicTime::from_micros(contributor.last_received_at_micros),
        ingress_basis(contributor.source_time_quality),
        contributor
            .uncertainty_micros
            .map_or(TimeUncertainty::Unknown, TimeUncertainty::Bounded),
        SourceTime::Unknown,
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
    RawProductIdentity::new(
        RAW_ID_NAMESPACE,
        format!(
            "product={product_id};file={:?};segments={};record={:?}/{:?};time={}",
            protocol.product_file_id(),
            protocol.segment_count(),
            protocol.record_index(),
            protocol.record_count(),
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
    /// A bounded output allocation failed.
    #[error("allocation failed for {operation}")]
    Allocation {
        /// Output being allocated.
        operation: &'static str,
        /// Allocation failure.
        #[source]
        source: TryReserveError,
    },
}
