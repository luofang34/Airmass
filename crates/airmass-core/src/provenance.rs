//! Weather source, origin, and delivery provenance.

use alloc::string::String;

use serde::{Deserialize, Serialize};

use crate::{SourceEpoch, SourceId};

/// One source and one continuous source instance.
#[derive(Clone, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
pub struct SourceRef {
    /// Stable source identity.
    pub id: SourceId,
    /// Continuous source instance.
    pub epoch: SourceEpoch,
}

impl SourceRef {
    /// Create a source reference.
    #[must_use]
    pub fn new(id: SourceId, epoch: SourceEpoch) -> Self {
        Self { id, epoch }
    }
}

/// What made weather evidence.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case", tag = "kind", content = "name")]
#[non_exhaustive]
pub enum WeatherOrigin {
    /// A local radio receiver made the evidence.
    LocalReceiver,
    /// Installed avionics made the evidence.
    InstalledAvionics,
    /// A network provider made the evidence.
    NetworkProvider,
    /// A recording supplied the evidence.
    Replay,
    /// A simulator made the evidence.
    Simulation,
    /// Another named origin made the evidence.
    Other(String),
}

/// Named path that delivered evidence to this runtime.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(transparent)]
pub struct DeliveryPath(String);

impl DeliveryPath {
    /// Create a delivery path name.
    #[must_use]
    pub fn new(path: impl Into<String>) -> Self {
        Self(path.into())
    }

    /// Get the delivery path name.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Identity that a source protocol assigns to a raw product.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct RawProductIdentity {
    /// Namespace that defines the identity.
    pub namespace: String,
    /// Identity text in that namespace.
    pub value: String,
}

impl RawProductIdentity {
    /// Create a raw product identity.
    #[must_use]
    pub fn new(namespace: impl Into<String>, value: impl Into<String>) -> Self {
        Self {
            namespace: namespace.into(),
            value: value.into(),
        }
    }
}

/// Provenance for one weather field.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ProductProvenance {
    /// Source and source epoch.
    pub source: SourceRef,
    /// Origin that made the evidence.
    pub origin: WeatherOrigin,
    /// Path that delivered the evidence.
    pub delivery_path: DeliveryPath,
    /// Raw source identity when the source supplies one.
    pub raw_product: Option<RawProductIdentity>,
}

impl ProductProvenance {
    /// Create product provenance.
    #[must_use]
    pub fn new(
        source: SourceRef,
        origin: WeatherOrigin,
        delivery_path: DeliveryPath,
        raw_product: Option<RawProductIdentity>,
    ) -> Self {
        Self {
            source,
            origin,
            delivery_path,
            raw_product,
        }
    }
}

/// Domain-owned quality evidence for one weather field.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case", tag = "state")]
pub enum QualityEvidence {
    /// The source supplies no quality value.
    Unavailable,
    /// The source supplies a named quality value.
    Reported {
        /// Scheme that defines the value.
        scheme: String,
        /// Value in the named scheme.
        value: String,
    },
}
