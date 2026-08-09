//! Typed errors for exported weather schema versions.

use thiserror::Error;

/// Failure to read a versioned weather record.
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
#[non_exhaustive]
pub enum WeatherSchemaError {
    /// The ingress record uses an unsupported version.
    #[error("unsupported weather ingress schema version {found}, supported version is {supported}")]
    UnsupportedIngressVersion {
        /// Version in the record.
        found: u16,
        /// Version that this library reads.
        supported: u16,
    },
    /// The snapshot record uses an unsupported version.
    #[error(
        "unsupported weather snapshot schema version {found}, supported version is {supported}"
    )]
    UnsupportedSnapshotVersion {
        /// Version in the record.
        found: u16,
        /// Version that this library reads.
        supported: u16,
    },
}
