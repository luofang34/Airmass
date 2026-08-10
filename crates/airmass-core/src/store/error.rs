//! Typed store failures with product and source context.

use thiserror::Error;

use crate::{
    AdvisoryValidationError, AdvisoryValidity, DurationMillis, MonotonicTime, ProducerInstanceId,
    SourceId, SourceProductRevision, SourceRef, UtcInterval, UtcTime, WeatherProductId,
    WeatherSchemaError,
};

/// Failure to validate or update weather lifecycle state.
#[derive(Clone, Debug, Eq, Error, PartialEq)]
#[non_exhaustive]
pub enum StoreError {
    /// A capacity value is not valid.
    #[error("store configuration value {name}={value} is not valid")]
    InvalidConfig {
        /// Configuration field name.
        name: &'static str,
        /// Rejected value.
        value: usize,
    },
    /// A source occurs more than once in the preference table.
    #[error("source {source_id:?} occurs more than once in the preference table")]
    DuplicateSourcePriority {
        /// Repeated source identity.
        source_id: SourceId,
    },
    /// The ingress schema is not supported.
    #[error("weather ingress for product {product_id:?} has an unsupported schema")]
    IngressSchema {
        /// Product identity from the record header.
        product_id: WeatherProductId,
        /// Typed schema failure.
        #[source]
        source: WeatherSchemaError,
    },
    /// The product capacity is full.
    #[error("product capacity {limit} rejects product {product_id:?}")]
    ProductCapacity {
        /// Rejected product identity.
        product_id: WeatherProductId,
        /// Configured product limit.
        limit: usize,
    },
    /// The source capacity for a product is full.
    #[error("source capacity {limit} rejects source {source_ref:?} for product {product_id:?}")]
    SourceCapacity {
        /// Product identity.
        product_id: WeatherProductId,
        /// Rejected source.
        source_ref: SourceRef,
        /// Configured source limit.
        limit: usize,
    },
    /// The payload is larger than the declared limit.
    #[error("payload for product {product_id:?} has {actual} bytes and exceeds limit {limit}")]
    PayloadTooLarge {
        /// Product identity.
        product_id: WeatherProductId,
        /// Payload byte length.
        actual: usize,
        /// Configured byte limit.
        limit: usize,
    },
    /// One advisory product does not contain exactly one lifecycle value.
    #[error("product {product_id:?} contains {actual} advisories; exactly 1 is required")]
    AdvisoryCount {
        /// Product identity.
        product_id: WeatherProductId,
        /// Supplied advisory count.
        actual: usize,
    },
    /// An advisory contains invalid source-derived structure.
    #[error("product {product_id:?} contains an invalid advisory")]
    InvalidAdvisory {
        /// Product identity.
        product_id: WeatherProductId,
        /// Typed advisory validation failure.
        #[source]
        source: AdvisoryValidationError,
    },
    /// Product and advisory validity do not identify one lifecycle interval.
    #[error(
        "product {product_id:?} has product validity {product_validity:?} and advisory validity {advisory_validity:?}"
    )]
    AdvisoryValidityMismatch {
        /// Product identity.
        product_id: WeatherProductId,
        /// Product lifecycle interval.
        product_validity: Option<UtcInterval>,
        /// Advisory field interval.
        advisory_validity: Option<AdvisoryValidity>,
    },
    /// A field names a different source than the event anchor.
    #[error(
        "field {field} for product {product_id:?} names source {found:?}, expected {expected:?}"
    )]
    FieldSourceMismatch {
        /// Product identity.
        product_id: WeatherProductId,
        /// Field name.
        field: &'static str,
        /// Source from the event anchor.
        expected: SourceRef,
        /// Source from the field.
        found: SourceRef,
    },
    /// Ingress contains evidence that only the store can make.
    #[error("field {field} for product {product_id:?} contains store lifecycle evidence")]
    IngressLifecycleEvidence {
        /// Product identity.
        product_id: WeatherProductId,
        /// Field name.
        field: &'static str,
    },
    /// A field ingress stamp is later than the operation time.
    #[error(
        "field {field} for product {product_id:?} has future monotonic ingress time {found:?}, current time is {now:?}"
    )]
    FutureIngressTime {
        /// Product identity.
        product_id: WeatherProductId,
        /// Field name.
        field: &'static str,
        /// Field ingress stamp.
        found: MonotonicTime,
        /// Operation monotonic time.
        now: MonotonicTime,
    },
    /// A product validity interval has an invalid order.
    #[error("product {product_id:?} has validity start {start:?} at or after end {end:?}")]
    InvalidValidity {
        /// Product identity.
        product_id: WeatherProductId,
        /// Validity start.
        start: UtcTime,
        /// Validity end.
        end: UtcTime,
    },
    /// The effective time is not before the validity end.
    #[error(
        "product {product_id:?} has effective time {effective_at:?} at or after validity end {valid_end:?}"
    )]
    EffectiveAfterValidity {
        /// Product identity.
        product_id: WeatherProductId,
        /// Product effective time.
        effective_at: UtcTime,
        /// Validity end.
        valid_end: UtcTime,
    },
    /// A monotonic expiry deadline is outside the clock range.
    #[error(
        "product {product_id:?} cannot add maximum age {maximum_age:?} to ingress time {ingress_time:?}"
    )]
    MonotonicExpiryOverflow {
        /// Product identity.
        product_id: WeatherProductId,
        /// Product ingress time.
        ingress_time: MonotonicTime,
        /// Source maximum age.
        maximum_age: DurationMillis,
    },
    /// An equal source revision has different data.
    #[error(
        "source {source_ref:?} supplied conflicting replay revision {revision:?} for product {product_id:?}"
    )]
    ConflictingReplay {
        /// Product identity.
        product_id: WeatherProductId,
        /// Source and epoch.
        source_ref: SourceRef,
        /// Conflicting source revision.
        revision: SourceProductRevision,
    },
    /// Civil UTC moved backwards.
    #[error("civil UTC moved backwards from {previous:?} to {supplied:?}")]
    UtcTimeRegression {
        /// Last accepted civil UTC value.
        previous: UtcTime,
        /// Rejected civil UTC value.
        supplied: UtcTime,
    },
    /// Monotonic time moved backwards.
    #[error("monotonic time moved backwards from {previous:?} to {supplied:?}")]
    MonotonicTimeRegression {
        /// Last accepted monotonic value.
        previous: MonotonicTime,
        /// Rejected monotonic value.
        supplied: MonotonicTime,
    },
    /// A product revision cannot advance without wrapping.
    #[error("product revision for {product_id:?} is exhausted")]
    ProductRevisionExhausted {
        /// Product whose revision is exhausted.
        product_id: WeatherProductId,
    },
    /// The snapshot revision cannot advance without wrapping.
    #[error("snapshot revision for producer {producer_instance_id:?} is exhausted")]
    SnapshotRevisionExhausted {
        /// Producer whose revision is exhausted.
        producer_instance_id: ProducerInstanceId,
    },
    /// Revision continuity has ended for this producer.
    #[error("weather producer {producer_instance_id:?} has ended")]
    ProducerEnded {
        /// Ended producer identity.
        producer_instance_id: ProducerInstanceId,
    },
}
