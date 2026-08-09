//! Versioned source-neutral weather ingress.

use alloc::boxed::Box;

use serde::{Deserialize, Serialize};

use crate::{
    Cancellation, ProductProvenance, SourceProductRevision, TimeEvidence, WeatherProduct,
    WeatherProductId, WeatherSchemaError,
};

/// Schema version written for weather ingress records.
pub const CURRENT_WEATHER_INGRESS_SCHEMA_VERSION: u16 = 2;

/// Product data or a cancellation from one source.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case", tag = "kind", content = "value")]
pub enum WeatherIngressEvent {
    /// A weather product revision.
    Product(Box<WeatherProduct>),
    /// A weather product cancellation.
    Cancellation(Box<Cancellation>),
}

impl WeatherIngressEvent {
    pub(crate) fn anchor_time(&self) -> TimeEvidence {
        match self {
            Self::Product(product) => product.payload().time,
            Self::Cancellation(cancellation) => cancellation.reason().time,
        }
    }

    pub(crate) fn anchor_provenance(&self) -> &ProductProvenance {
        match self {
            Self::Product(product) => &product.payload().provenance,
            Self::Cancellation(cancellation) => &cancellation.reason().provenance,
        }
    }
}

/// One versioned source-neutral weather input.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct WeatherIngress {
    schema_version: u16,
    product_id: WeatherProductId,
    source_product_revision: SourceProductRevision,
    event: WeatherIngressEvent,
}

impl WeatherIngress {
    /// Create a weather product ingress record.
    #[must_use]
    pub fn product(
        product_id: WeatherProductId,
        source_product_revision: SourceProductRevision,
        product: WeatherProduct,
    ) -> Self {
        Self::new(
            product_id,
            source_product_revision,
            WeatherIngressEvent::Product(Box::new(product)),
        )
    }

    /// Create a weather cancellation ingress record.
    #[must_use]
    pub fn cancellation(
        product_id: WeatherProductId,
        source_product_revision: SourceProductRevision,
        cancellation: Cancellation,
    ) -> Self {
        Self::new(
            product_id,
            source_product_revision,
            WeatherIngressEvent::Cancellation(Box::new(cancellation)),
        )
    }

    fn new(
        product_id: WeatherProductId,
        source_product_revision: SourceProductRevision,
        event: WeatherIngressEvent,
    ) -> Self {
        Self {
            schema_version: CURRENT_WEATHER_INGRESS_SCHEMA_VERSION,
            product_id,
            source_product_revision,
            event,
        }
    }

    /// Get the declared schema version.
    #[must_use]
    pub const fn schema_version(&self) -> u16 {
        self.schema_version
    }

    /// Get the weather product identity.
    #[must_use]
    pub const fn product_id(&self) -> &WeatherProductId {
        &self.product_id
    }

    /// Get the source product revision.
    #[must_use]
    pub const fn source_product_revision(&self) -> SourceProductRevision {
        self.source_product_revision
    }

    /// Borrow the event after version validation.
    pub fn event(&self) -> Result<&WeatherIngressEvent, WeatherSchemaError> {
        validate_version(self.schema_version)?;
        Ok(&self.event)
    }

    /// Take the event after version validation.
    pub fn into_event(self) -> Result<WeatherIngressEvent, WeatherSchemaError> {
        validate_version(self.schema_version)?;
        Ok(self.event)
    }
}

/// Header that a reader decodes before an ingress payload.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq)]
pub struct WeatherIngressHeader {
    schema_version: u16,
}

impl WeatherIngressHeader {
    /// Get the declared schema version.
    #[must_use]
    pub const fn schema_version(self) -> u16 {
        self.schema_version
    }

    /// Reject an unsupported ingress version.
    pub fn validate(self) -> Result<(), WeatherSchemaError> {
        validate_version(self.schema_version)
    }
}

fn validate_version(found: u16) -> Result<(), WeatherSchemaError> {
    if found == CURRENT_WEATHER_INGRESS_SCHEMA_VERSION {
        Ok(())
    } else {
        Err(WeatherSchemaError::UnsupportedIngressVersion {
            found,
            supported: CURRENT_WEATHER_INGRESS_SCHEMA_VERSION,
        })
    }
}
