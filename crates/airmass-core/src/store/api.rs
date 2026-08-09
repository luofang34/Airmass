//! Public store configuration and operation results.

use alloc::vec::Vec;

use crate::{DeadlineSet, SourceId, WeatherSnapshotHandle};

/// Configured preference for one weather source.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SourcePriority {
    source_id: SourceId,
    rank: u16,
}

impl SourcePriority {
    /// Create a source preference. A lower rank has more preference.
    #[must_use]
    pub fn new(source_id: SourceId, rank: u16) -> Self {
        Self { source_id, rank }
    }

    /// Get the source identity.
    #[must_use]
    pub const fn source_id(&self) -> &SourceId {
        &self.source_id
    }

    /// Get the source rank.
    #[must_use]
    pub const fn rank(&self) -> u16 {
        self.rank
    }
}

/// Fixed capacity and source preference configuration.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StoreConfig {
    /// Maximum number of product identities.
    pub max_products: usize,
    /// Maximum number of sources for each product.
    pub max_sources_per_product: usize,
    /// Maximum byte length of one opaque payload.
    pub max_payload_bytes: usize,
    /// Stable source preference table.
    pub source_priorities: Vec<SourcePriority>,
}

impl Default for StoreConfig {
    fn default() -> Self {
        Self {
            max_products: 1_024,
            max_sources_per_product: 8,
            max_payload_bytes: 1_048_576,
            source_priorities: Vec::new(),
        }
    }
}

impl StoreConfig {
    pub(crate) fn source_rank(&self, source_id: &SourceId) -> u16 {
        self.source_priorities
            .iter()
            .find(|priority| priority.source_id() == source_id)
            .map_or(u16::MAX, SourcePriority::rank)
    }
}

/// Result of source revision processing.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum AcceptStatus {
    /// The store applied new source evidence.
    Applied,
    /// The store had already applied the exact source revision.
    Replay,
    /// A newer source revision or epoch already exists.
    Stale,
}

/// Result of one ingress operation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AcceptOutcome {
    pub(super) status: AcceptStatus,
    pub(super) publication: Option<WeatherSnapshotHandle>,
    pub(super) next_deadline: DeadlineSet,
}

impl AcceptOutcome {
    /// Get the source revision result.
    #[must_use]
    pub const fn status(&self) -> AcceptStatus {
        self.status
    }

    /// Get a new publication when the visible state changed.
    #[must_use]
    pub const fn publication(&self) -> Option<&WeatherSnapshotHandle> {
        self.publication.as_ref()
    }

    /// Take a new publication when the visible state changed.
    #[must_use]
    pub fn into_publication(self) -> Option<WeatherSnapshotHandle> {
        self.publication
    }

    /// Get the next time when the visible state can change.
    #[must_use]
    pub const fn next_deadline(&self) -> DeadlineSet {
        self.next_deadline
    }
}

/// Result of one explicit time advance.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AdvanceOutcome {
    pub(super) publication: Option<WeatherSnapshotHandle>,
    pub(super) next_deadline: DeadlineSet,
}

impl AdvanceOutcome {
    /// Get a new publication when the visible state changed.
    #[must_use]
    pub const fn publication(&self) -> Option<&WeatherSnapshotHandle> {
        self.publication.as_ref()
    }

    /// Take a new publication when the visible state changed.
    #[must_use]
    pub fn into_publication(self) -> Option<WeatherSnapshotHandle> {
        self.publication
    }

    /// Get the next time when the visible state can change.
    #[must_use]
    pub const fn next_deadline(&self) -> DeadlineSet {
        self.next_deadline
    }
}
