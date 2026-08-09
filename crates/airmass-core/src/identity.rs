//! Identities and revision counters for weather state.

use alloc::string::String;

use serde::{Deserialize, Serialize};

/// Identity of one continuous Airmass producer instance.
///
/// The application supplies a new value for each producer instance. A source
/// restart does not change this value.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
#[serde(transparent)]
pub struct ProducerInstanceId(u64);

impl ProducerInstanceId {
    /// Create an identity from an application-assigned value.
    #[must_use]
    pub const fn new(id: u64) -> Self {
        Self(id)
    }

    /// Get the application-assigned value.
    #[must_use]
    pub const fn get(self) -> u64 {
        self.0
    }
}

/// Revision of the complete published weather snapshot.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(transparent)]
pub struct SnapshotRevision(u64);

impl SnapshotRevision {
    /// Create a revision from a stored value.
    #[must_use]
    pub const fn new(revision: u64) -> Self {
        Self(revision)
    }

    /// Get the numeric revision.
    #[must_use]
    pub const fn get(self) -> u64 {
        self.0
    }

    pub(crate) fn next(self) -> Option<Self> {
        let next = self.0.wrapping_add(1);
        (next != 0).then_some(Self(next))
    }
}

/// Revision of one published weather product state.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(transparent)]
pub struct ProductRevision(u64);

impl ProductRevision {
    /// Create a revision from a stored value.
    #[must_use]
    pub const fn new(revision: u64) -> Self {
        Self(revision)
    }

    /// Get the numeric revision.
    #[must_use]
    pub const fn get(self) -> u64 {
        self.0
    }

    pub(crate) fn next(self) -> Option<Self> {
        let next = self.0.wrapping_add(1);
        (next != 0).then_some(Self(next))
    }
}

/// Revision supplied by one source for one product and source epoch.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(transparent)]
pub struct SourceProductRevision(u64);

impl SourceProductRevision {
    /// Create a source revision.
    #[must_use]
    pub const fn new(revision: u64) -> Self {
        Self(revision)
    }

    /// Get the numeric revision.
    #[must_use]
    pub const fn get(self) -> u64 {
        self.0
    }
}

/// Identity of one weather source.
#[derive(Clone, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(transparent)]
pub struct SourceId(String);

impl SourceId {
    /// Create a source identity.
    #[must_use]
    pub fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }

    /// Get the source identity text.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Identity of one continuous source instance.
///
/// A source assigns a higher value after each restart.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(transparent)]
pub struct SourceEpoch(u64);

impl SourceEpoch {
    /// Create a source epoch from a source-assigned value.
    #[must_use]
    pub const fn new(epoch: u64) -> Self {
        Self(epoch)
    }

    /// Get the numeric epoch.
    #[must_use]
    pub const fn get(self) -> u64 {
        self.0
    }
}

/// Source-neutral identity of one weather product.
#[derive(Clone, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(transparent)]
pub struct WeatherProductId(String);

impl WeatherProductId {
    /// Create a weather product identity.
    #[must_use]
    pub fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }

    /// Get the weather product identity text.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}
