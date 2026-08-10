//! Store configuration and weather ingress validation.

use crate::{
    AbsenceEvidence, FieldState, StoreConfig, StoreError, TextWeatherReport, TimeEvidence,
    TimedField, WeatherAdvisory, WeatherIngressEvent, WeatherProduct, WeatherProductId,
    WeatherValue,
};

pub(super) fn validate_config(config: &StoreConfig) -> Result<(), StoreError> {
    validate_nonzero("max_products", config.max_products)?;
    validate_nonzero("max_sources_per_product", config.max_sources_per_product)?;
    validate_nonzero("max_payload_bytes", config.max_payload_bytes)?;
    if config.max_sources_per_product > usize::from(u16::MAX) {
        return Err(StoreError::InvalidConfig {
            name: "max_sources_per_product",
            value: config.max_sources_per_product,
        });
    }
    for (index, priority) in config.source_priorities.iter().enumerate() {
        if config.source_priorities[..index]
            .iter()
            .any(|current| current.source_id() == priority.source_id())
        {
            return Err(StoreError::DuplicateSourcePriority {
                source_id: priority.source_id().clone(),
            });
        }
    }
    Ok(())
}

fn validate_nonzero(name: &'static str, value: usize) -> Result<(), StoreError> {
    if value == 0 {
        Err(StoreError::InvalidConfig { name, value })
    } else {
        Ok(())
    }
}

pub(super) fn validate_ingress(
    product_id: &WeatherProductId,
    event: &WeatherIngressEvent,
    now: crate::EvaluationTime,
    config: &StoreConfig,
) -> Result<(), StoreError> {
    let expected = &event.anchor_provenance().source;
    match event {
        WeatherIngressEvent::Product(product) => {
            if product.payload().value.bytes.len() > config.max_payload_bytes {
                return Err(StoreError::PayloadTooLarge {
                    product_id: product_id.clone(),
                    actual: product.payload().value.bytes.len(),
                    limit: config.max_payload_bytes,
                });
            }
            validate_product(product_id, product, expected, now)?;
        }
        WeatherIngressEvent::Cancellation(cancellation) => {
            validate_timed(
                product_id,
                "cancellation.reason",
                cancellation.reason(),
                expected,
                now,
            )?;
            validate_field(
                product_id,
                "cancellation.effective_at",
                cancellation.effective_at(),
                expected,
                now,
            )?;
        }
    }
    Ok(())
}

fn validate_product(
    product_id: &WeatherProductId,
    product: &WeatherProduct,
    expected: &crate::SourceRef,
    now: crate::EvaluationTime,
) -> Result<(), StoreError> {
    validate_timed(product_id, "payload", product.payload(), expected, now)?;
    validate_typed_value(product_id, product, expected, now)?;
    validate_field(
        product_id,
        "issue_time",
        product.issue_time(),
        expected,
        now,
    )?;
    validate_field(
        product_id,
        "effective_at",
        product.effective_at(),
        expected,
        now,
    )?;
    validate_field(
        product_id,
        "valid_period",
        product.valid_period(),
        expected,
        now,
    )?;
    validate_field(
        product_id,
        "maximum_age",
        product.maximum_age(),
        expected,
        now,
    )?;
    validate_product_times(product_id, product)
}

fn validate_typed_value(
    product_id: &WeatherProductId,
    product: &WeatherProduct,
    expected: &crate::SourceRef,
    now: crate::EvaluationTime,
) -> Result<(), StoreError> {
    let value = product.typed_value();
    validate_field(product_id, "typed_value", value, expected, now)?;
    let Some(field) = value.as_present() else {
        return Ok(());
    };
    match &field.value {
        WeatherValue::TextReports(reports) => {
            for report in reports {
                validate_text_report(product_id, report, expected, now)?;
            }
        }
        WeatherValue::Advisories(advisories) => {
            let [advisory] = advisories.as_slice() else {
                return Err(StoreError::AdvisoryCount {
                    product_id: product_id.clone(),
                    actual: advisories.len(),
                });
            };
            validate_advisory(product_id, advisory, expected, now)?;
            validate_advisory_validity(product_id, product, advisory)?;
        }
    }
    Ok(())
}

fn validate_advisory_validity(
    product_id: &WeatherProductId,
    product: &WeatherProduct,
    advisory: &WeatherAdvisory,
) -> Result<(), StoreError> {
    let product_validity = product.valid_period().as_present().map(|field| field.value);
    let advisory_validity = advisory.validity().as_present().map(|field| field.value);
    let advisory_lifecycle = advisory_validity
        .map(|validity| validity.lifecycle_interval())
        .transpose()
        .map_err(|source| StoreError::InvalidAdvisory {
            product_id: product_id.clone(),
            source,
        })?;
    if advisory_validity.is_some() && product_validity == advisory_lifecycle {
        Ok(())
    } else {
        Err(StoreError::AdvisoryValidityMismatch {
            product_id: product_id.clone(),
            product_validity,
            advisory_validity,
        })
    }
}

fn validate_advisory(
    product_id: &WeatherProductId,
    advisory: &WeatherAdvisory,
    expected: &crate::SourceRef,
    now: crate::EvaluationTime,
) -> Result<(), StoreError> {
    advisory
        .validate_structure()
        .map_err(|source| StoreError::InvalidAdvisory {
            product_id: product_id.clone(),
            source,
        })?;
    validate_field(
        product_id,
        "advisory.identity",
        advisory.identity(),
        expected,
        now,
    )?;
    validate_field(
        product_id,
        "advisory.type",
        advisory.advisory_type(),
        expected,
        now,
    )?;
    validate_field(
        product_id,
        "advisory.validity",
        advisory.validity(),
        expected,
        now,
    )?;
    validate_field(
        product_id,
        "advisory.altitude",
        advisory.altitude(),
        expected,
        now,
    )?;
    validate_field(
        product_id,
        "advisory.geometry",
        advisory.geometry(),
        expected,
        now,
    )?;
    validate_field(
        product_id,
        "advisory.altitude_band",
        advisory.altitude_band(),
        expected,
        now,
    )
}

fn validate_text_report(
    product_id: &WeatherProductId,
    report: &TextWeatherReport,
    expected: &crate::SourceRef,
    now: crate::EvaluationTime,
) -> Result<(), StoreError> {
    validate_field(
        product_id,
        "report.type",
        report.report_type(),
        expected,
        now,
    )?;
    validate_field(
        product_id,
        "report.station",
        report.station_id(),
        expected,
        now,
    )?;
    validate_field(
        product_id,
        "report.time",
        report.report_time(),
        expected,
        now,
    )?;
    validate_timed(product_id, "report.text", report.text(), expected, now)?;
    validate_field(
        product_id,
        "report.flight_category",
        report.flight_category(),
        expected,
        now,
    )
}

fn validate_product_times(
    product_id: &WeatherProductId,
    product: &WeatherProduct,
) -> Result<(), StoreError> {
    let validity = product.valid_period().as_present().map(|field| field.value);
    if let Some(interval) = validity {
        if !interval.has_valid_order() {
            return Err(StoreError::InvalidValidity {
                product_id: product_id.clone(),
                start: interval.start,
                end: interval.end,
            });
        }
        if let Some(effective) = product.effective_at().as_present()
            && effective.value >= interval.end
        {
            return Err(StoreError::EffectiveAfterValidity {
                product_id: product_id.clone(),
                effective_at: effective.value,
                valid_end: interval.end,
            });
        }
    }
    if let Some(maximum_age) = product.maximum_age().as_present() {
        let ingress_time = product.payload().time.ingress_monotonic;
        if ingress_time.checked_add(maximum_age.value).is_none() {
            return Err(StoreError::MonotonicExpiryOverflow {
                product_id: product_id.clone(),
                ingress_time,
                maximum_age: maximum_age.value,
            });
        }
    }
    Ok(())
}

fn validate_field<T>(
    product_id: &WeatherProductId,
    name: &'static str,
    field: &FieldState<T>,
    expected: &crate::SourceRef,
    now: crate::EvaluationTime,
) -> Result<(), StoreError> {
    match field {
        FieldState::Present(field) => validate_timed(product_id, name, field, expected, now),
        FieldState::Absent(absent) => match absent.evidence() {
            AbsenceEvidence::Unknown => Ok(()),
            AbsenceEvidence::Source {
                time, provenance, ..
            } => validate_evidence(product_id, name, *time, provenance, expected, now),
            AbsenceEvidence::Lifecycle { .. } => Err(StoreError::IngressLifecycleEvidence {
                product_id: product_id.clone(),
                field: name,
            }),
        },
    }
}

fn validate_timed<T>(
    product_id: &WeatherProductId,
    name: &'static str,
    field: &TimedField<T>,
    expected: &crate::SourceRef,
    now: crate::EvaluationTime,
) -> Result<(), StoreError> {
    validate_evidence(
        product_id,
        name,
        field.time,
        &field.provenance,
        expected,
        now,
    )
}

fn validate_evidence(
    product_id: &WeatherProductId,
    name: &'static str,
    time: TimeEvidence,
    provenance: &crate::ProductProvenance,
    expected: &crate::SourceRef,
    now: crate::EvaluationTime,
) -> Result<(), StoreError> {
    if &provenance.source != expected {
        return Err(StoreError::FieldSourceMismatch {
            product_id: product_id.clone(),
            field: name,
            expected: expected.clone(),
            found: provenance.source.clone(),
        });
    }
    if time.ingress_monotonic > now.monotonic {
        return Err(StoreError::FutureIngressTime {
            product_id: product_id.clone(),
            field: name,
            found: time.ingress_monotonic,
            now: now.monotonic,
        });
    }
    Ok(())
}
