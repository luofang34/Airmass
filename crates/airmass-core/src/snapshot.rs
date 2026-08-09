//! Immutable and versioned weather snapshots.

use alloc::{boxed::Box, vec::Vec};
use core::ops::Deref;

use serde::{Deserialize, Serialize};

use crate::{
    AbsentField, Cancellation, ProducerInstanceId, ProductRevision, SnapshotRevision,
    SourceProductRevision, SourceRef, WeatherProduct, WeatherProductId, WeatherSchemaError,
};

/// Schema version written for exported weather snapshots.
pub const CURRENT_WEATHER_SNAPSHOT_SCHEMA_VERSION: u16 = 3;

/// Rule that decided between weather source candidates.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum SelectionCriterion {
    /// Only one candidate was eligible.
    SoleSource,
    /// A configured source rank decided the result.
    ConfiguredPriority,
    /// Compatible source time evidence decided the result.
    KnownSourceTime,
    /// Monotonic ingress order decided the result.
    IngressOrder,
    /// Stable source identity order decided the result.
    StableSourceIdentity,
}

/// Explanation of one deterministic source selection.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct SelectionExplanation {
    /// Source that supplied the published state.
    pub selected_source: SourceRef,
    /// Number of candidates in the selection.
    pub considered_sources: u16,
    /// Rule that selected the source.
    pub decisive_criterion: SelectionCriterion,
}

/// Current lifecycle state of one weather product.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case", tag = "state")]
pub enum WeatherProductState {
    /// The product value is current.
    Available {
        /// Current source-derived product fields.
        product: Box<WeatherProduct>,
    },
    /// Source evidence cancelled the product.
    Cancelled {
        /// Explicit absent state for the product value.
        absence: AbsentField,
        /// Source-derived cancellation evidence.
        cancellation: Box<Cancellation>,
    },
    /// A lifecycle condition makes the value absent.
    Absent {
        /// Explicit absent state for the product value.
        absence: AbsentField,
    },
}

impl WeatherProductState {
    /// Get an available product.
    #[must_use]
    pub fn available(&self) -> Option<&WeatherProduct> {
        match self {
            Self::Available { product } => Some(product.as_ref()),
            Self::Cancelled { .. } | Self::Absent { .. } => None,
        }
    }

    /// Get the absent field when the product is not available.
    #[must_use]
    pub const fn absence(&self) -> Option<&AbsentField> {
        match self {
            Self::Available { .. } => None,
            Self::Cancelled { absence, .. } | Self::Absent { absence } => Some(absence),
        }
    }
}

/// One product in an immutable weather snapshot.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct WeatherProductSnapshot {
    product_id: WeatherProductId,
    product_revision: ProductRevision,
    source_product_revision: SourceProductRevision,
    state: WeatherProductState,
    selection: SelectionExplanation,
}

impl WeatherProductSnapshot {
    pub(crate) fn new(
        product_id: WeatherProductId,
        product_revision: ProductRevision,
        source_product_revision: SourceProductRevision,
        state: WeatherProductState,
        selection: SelectionExplanation,
    ) -> Self {
        Self {
            product_id,
            product_revision,
            source_product_revision,
            state,
            selection,
        }
    }

    /// Get the source-neutral product identity.
    #[must_use]
    pub const fn product_id(&self) -> &WeatherProductId {
        &self.product_id
    }

    /// Get the Airmass product revision.
    #[must_use]
    pub const fn product_revision(&self) -> ProductRevision {
        self.product_revision
    }

    /// Get the selected source product revision.
    #[must_use]
    pub const fn source_product_revision(&self) -> SourceProductRevision {
        self.source_product_revision
    }

    /// Get the current product state.
    #[must_use]
    pub const fn state(&self) -> &WeatherProductState {
        &self.state
    }

    /// Get the source selection explanation.
    #[must_use]
    pub const fn selection(&self) -> &SelectionExplanation {
        &self.selection
    }
}

/// Complete immutable weather state from one producer.
#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct WeatherSnapshot {
    products: Vec<WeatherProductSnapshot>,
}

impl WeatherSnapshot {
    pub(crate) fn new(products: Vec<WeatherProductSnapshot>) -> Self {
        Self { products }
    }

    /// Get all product states in product identity order.
    #[must_use]
    pub fn products(&self) -> &[WeatherProductSnapshot] {
        &self.products
    }

    /// Find one product state.
    #[must_use]
    pub fn product(&self, product_id: &WeatherProductId) -> Option<&WeatherProductSnapshot> {
        self.products
            .binary_search_by(|product| product.product_id.cmp(product_id))
            .ok()
            .map(|index| &self.products[index])
    }
}

/// Identity, revision, and immutable weather state from one publication.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct WeatherSnapshotEnvelope {
    producer_instance_id: ProducerInstanceId,
    snapshot_revision: SnapshotRevision,
    snapshot: WeatherSnapshot,
}

impl WeatherSnapshotEnvelope {
    /// Create a snapshot envelope.
    #[must_use]
    pub const fn new(
        producer_instance_id: ProducerInstanceId,
        snapshot_revision: SnapshotRevision,
        snapshot: WeatherSnapshot,
    ) -> Self {
        Self {
            producer_instance_id,
            snapshot_revision,
            snapshot,
        }
    }

    /// Get the producer instance identity.
    #[must_use]
    pub const fn producer_instance_id(&self) -> ProducerInstanceId {
        self.producer_instance_id
    }

    /// Get the complete snapshot revision.
    #[must_use]
    pub const fn snapshot_revision(&self) -> SnapshotRevision {
        self.snapshot_revision
    }

    /// Get the immutable weather state.
    #[must_use]
    pub const fn snapshot(&self) -> &WeatherSnapshot {
        &self.snapshot
    }
}

/// Retainable immutable handle for one weather publication.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WeatherSnapshotHandle {
    envelope: WeatherSnapshotEnvelope,
}

impl WeatherSnapshotHandle {
    pub(crate) const fn new(envelope: WeatherSnapshotEnvelope) -> Self {
        Self { envelope }
    }

    /// Get the complete snapshot envelope.
    #[must_use]
    pub const fn envelope(&self) -> &WeatherSnapshotEnvelope {
        &self.envelope
    }

    /// Take the complete snapshot envelope.
    #[must_use]
    pub fn into_envelope(self) -> WeatherSnapshotEnvelope {
        self.envelope
    }
}

impl AsRef<WeatherSnapshotEnvelope> for WeatherSnapshotHandle {
    fn as_ref(&self) -> &WeatherSnapshotEnvelope {
        &self.envelope
    }
}

impl Deref for WeatherSnapshotHandle {
    type Target = WeatherSnapshotEnvelope;

    fn deref(&self) -> &Self::Target {
        &self.envelope
    }
}

/// One versioned exported weather snapshot.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct WeatherSnapshotRecord {
    schema_version: u16,
    envelope: WeatherSnapshotEnvelope,
}

impl WeatherSnapshotRecord {
    /// Create a record with the current schema version.
    #[must_use]
    pub const fn new(envelope: WeatherSnapshotEnvelope) -> Self {
        Self {
            schema_version: CURRENT_WEATHER_SNAPSHOT_SCHEMA_VERSION,
            envelope,
        }
    }

    /// Get the declared schema version.
    #[must_use]
    pub const fn schema_version(&self) -> u16 {
        self.schema_version
    }

    /// Get the envelope after version validation.
    pub fn envelope(&self) -> Result<&WeatherSnapshotEnvelope, WeatherSchemaError> {
        validate_version(self.schema_version)?;
        Ok(&self.envelope)
    }

    /// Take the envelope after version validation.
    pub fn into_envelope(self) -> Result<WeatherSnapshotEnvelope, WeatherSchemaError> {
        validate_version(self.schema_version)?;
        Ok(self.envelope)
    }
}

/// Header that a reader decodes before a snapshot payload.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq)]
pub struct WeatherSnapshotRecordHeader {
    schema_version: u16,
}

impl WeatherSnapshotRecordHeader {
    /// Get the declared schema version.
    #[must_use]
    pub const fn schema_version(self) -> u16 {
        self.schema_version
    }

    /// Reject an unsupported snapshot version.
    pub fn validate(self) -> Result<(), WeatherSchemaError> {
        validate_version(self.schema_version)
    }
}

fn validate_version(found: u16) -> Result<(), WeatherSchemaError> {
    if found == CURRENT_WEATHER_SNAPSHOT_SCHEMA_VERSION {
        Ok(())
    } else {
        Err(WeatherSchemaError::UnsupportedSnapshotVersion {
            found,
            supported: CURRENT_WEATHER_SNAPSHOT_SCHEMA_VERSION,
        })
    }
}
