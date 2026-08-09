//! Synchronous single-writer weather lifecycle store.

use alloc::collections::BTreeMap;

use crate::{
    DeadlineSet, EvaluationTime, ProducerInstanceId, ProductRevision, SnapshotRevision,
    SourceEpoch, SourceId, SourceProductRevision, SourceRef, WeatherIngress, WeatherIngressEvent,
    WeatherProductId, WeatherProductSnapshot, WeatherSnapshot, WeatherSnapshotEnvelope,
    WeatherSnapshotHandle,
};

mod api;
mod error;
mod lifecycle;
mod selection;
mod validation;

pub use api::{AcceptOutcome, AcceptStatus, AdvanceOutcome, SourcePriority, StoreConfig};
pub use error::StoreError;

#[cfg(test)]
mod tests;

/// Source-neutral weather state with one synchronous writer.
#[derive(Debug)]
pub struct WeatherStore {
    config: StoreConfig,
    producer_instance_id: ProducerInstanceId,
    snapshot_revision: SnapshotRevision,
    products: BTreeMap<WeatherProductId, ProductEntry>,
    current: WeatherSnapshotHandle,
    last_time: Option<EvaluationTime>,
    ended: bool,
}

impl WeatherStore {
    /// Create an empty store for one producer instance.
    pub fn new(
        config: StoreConfig,
        producer_instance_id: ProducerInstanceId,
    ) -> Result<Self, StoreError> {
        validation::validate_config(&config)?;
        let envelope = WeatherSnapshotEnvelope::new(
            producer_instance_id,
            SnapshotRevision::new(0),
            WeatherSnapshot::default(),
        );
        Ok(Self {
            config,
            producer_instance_id,
            snapshot_revision: SnapshotRevision::new(0),
            products: BTreeMap::new(),
            current: WeatherSnapshotHandle::new(envelope),
            last_time: None,
            ended: false,
        })
    }

    /// Accept one source-neutral product event.
    pub fn accept(
        &mut self,
        ingress: WeatherIngress,
        now: EvaluationTime,
    ) -> Result<AcceptOutcome, StoreError> {
        self.ensure_active()?;
        let product_id = ingress.product_id().clone();
        let source_revision = ingress.source_product_revision();
        let event = ingress
            .into_event()
            .map_err(|source| StoreError::IngressSchema {
                product_id: product_id.clone(),
                source,
            })?;
        validation::validate_ingress(&product_id, &event, now, &self.config)?;
        let source = event.anchor_provenance().source.clone();
        let status = self.classify(&product_id, &source, source_revision, &event)?;
        self.validate_time(now)?;
        self.last_time = Some(now);
        lifecycle::promote_pending(&mut self.products, now);
        if status == AcceptStatus::Applied {
            self.apply(product_id, source_revision, event, now);
        }
        let publication = self.publish(now)?;
        let next_deadline = lifecycle::next_deadline(&self.products, now);
        Ok(AcceptOutcome {
            status,
            publication,
            next_deadline,
        })
    }

    /// Apply time without accepting source input.
    pub fn advance_time(&mut self, now: EvaluationTime) -> Result<AdvanceOutcome, StoreError> {
        self.ensure_active()?;
        self.validate_time(now)?;
        self.last_time = Some(now);
        lifecycle::promote_pending(&mut self.products, now);
        let publication = self.publish(now)?;
        let next_deadline = lifecycle::next_deadline(&self.products, now);
        Ok(AdvanceOutcome {
            publication,
            next_deadline,
        })
    }

    /// Capture one complete immutable publication.
    #[must_use]
    pub fn capture(&self) -> WeatherSnapshotHandle {
        self.current.clone()
    }

    /// Get the next time when the visible state can change.
    #[must_use]
    pub fn next_deadline(&self) -> DeadlineSet {
        self.last_time.map_or_else(DeadlineSet::default, |now| {
            lifecycle::next_deadline(&self.products, now)
        })
    }

    fn ensure_active(&self) -> Result<(), StoreError> {
        if self.ended {
            Err(StoreError::ProducerEnded {
                producer_instance_id: self.producer_instance_id,
            })
        } else {
            Ok(())
        }
    }

    fn validate_time(&self, now: EvaluationTime) -> Result<(), StoreError> {
        let Some(previous) = self.last_time else {
            return Ok(());
        };
        if now.utc < previous.utc {
            return Err(StoreError::UtcTimeRegression {
                previous: previous.utc,
                supplied: now.utc,
            });
        }
        if now.monotonic < previous.monotonic {
            return Err(StoreError::MonotonicTimeRegression {
                previous: previous.monotonic,
                supplied: now.monotonic,
            });
        }
        Ok(())
    }

    fn classify(
        &self,
        product_id: &WeatherProductId,
        source: &SourceRef,
        revision: SourceProductRevision,
        event: &WeatherIngressEvent,
    ) -> Result<AcceptStatus, StoreError> {
        let Some(entry) = self.products.get(product_id) else {
            if self.products.len() >= self.config.max_products {
                return Err(StoreError::ProductCapacity {
                    product_id: product_id.clone(),
                    limit: self.config.max_products,
                });
            }
            return Ok(AcceptStatus::Applied);
        };
        let Some(slot) = entry.sources.get(&source.id) else {
            if entry.sources.len() >= self.config.max_sources_per_product {
                return Err(StoreError::SourceCapacity {
                    product_id: product_id.clone(),
                    source_ref: source.clone(),
                    limit: self.config.max_sources_per_product,
                });
            }
            return Ok(AcceptStatus::Applied);
        };
        classify_slot(product_id, source, revision, event, slot)
    }

    fn apply(
        &mut self,
        product_id: WeatherProductId,
        source_revision: SourceProductRevision,
        event: WeatherIngressEvent,
        now: EvaluationTime,
    ) {
        let source = event.anchor_provenance().source.clone();
        let candidate = Candidate {
            source_product_revision: source_revision,
            event,
        };
        let entry = self
            .products
            .entry(product_id)
            .or_insert_with(ProductEntry::new);
        let slot = entry
            .sources
            .entry(source.id.clone())
            .or_insert_with(|| SourceSlot::new(source.epoch));
        if source.epoch > slot.epoch {
            *slot = SourceSlot::new(source.epoch);
        }
        slot.place(candidate, now);
    }

    fn publish(
        &mut self,
        now: EvaluationTime,
    ) -> Result<Option<WeatherSnapshotHandle>, StoreError> {
        let mut changed = false;
        for (product_id, entry) in &mut self.products {
            let Some(body) = lifecycle::published_body(entry, &self.config, now) else {
                continue;
            };
            if entry.published.as_ref() != Some(&body) {
                let Some(next) = entry.product_revision.next() else {
                    self.ended = true;
                    return Err(StoreError::ProductRevisionExhausted {
                        product_id: product_id.clone(),
                    });
                };
                entry.product_revision = next;
                entry.published = Some(body);
                changed = true;
            }
        }
        if !changed {
            return Ok(None);
        }
        self.publish_snapshot()
    }

    fn publish_snapshot(&mut self) -> Result<Option<WeatherSnapshotHandle>, StoreError> {
        let Some(snapshot_revision) = self.snapshot_revision.next() else {
            self.ended = true;
            return Err(StoreError::SnapshotRevisionExhausted {
                producer_instance_id: self.producer_instance_id,
            });
        };
        let products = self
            .products
            .iter()
            .filter_map(|(product_id, entry)| entry.snapshot(product_id.clone()))
            .collect();
        let snapshot = WeatherSnapshot::new(products);
        let envelope =
            WeatherSnapshotEnvelope::new(self.producer_instance_id, snapshot_revision, snapshot);
        let handle = WeatherSnapshotHandle::new(envelope);
        self.snapshot_revision = snapshot_revision;
        self.current = handle.clone();
        Ok(Some(handle))
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct Candidate {
    pub(super) source_product_revision: SourceProductRevision,
    pub(super) event: WeatherIngressEvent,
}

impl Candidate {
    pub(super) fn source(&self) -> &SourceRef {
        &self.event.anchor_provenance().source
    }

    pub(super) fn comparison_time(&self) -> Option<(crate::UtcTime, crate::SourceTimeKind)> {
        if let WeatherIngressEvent::Product(product) = &self.event
            && let Some(issue_time) = product.issue_time().as_present()
        {
            return Some((issue_time.value, crate::SourceTimeKind::Issue));
        }
        match self.event.anchor_time().source {
            crate::SourceTime::Known { value, kind, .. } => Some((value, kind)),
            crate::SourceTime::Unknown => None,
        }
    }
}

#[derive(Clone, Debug)]
pub(super) struct SourceSlot {
    pub(super) epoch: SourceEpoch,
    pub(super) current: Option<Candidate>,
    pub(super) pending: Option<Candidate>,
}

impl SourceSlot {
    fn new(epoch: SourceEpoch) -> Self {
        Self {
            epoch,
            current: None,
            pending: None,
        }
    }

    fn latest(&self) -> Option<&Candidate> {
        match (&self.current, &self.pending) {
            (Some(current), Some(pending)) => {
                if current.source_product_revision > pending.source_product_revision {
                    Some(current)
                } else {
                    Some(pending)
                }
            }
            (Some(current), None) => Some(current),
            (None, Some(pending)) => Some(pending),
            (None, None) => None,
        }
    }

    fn place(&mut self, candidate: Candidate, now: EvaluationTime) {
        if lifecycle::is_future(&candidate, now.utc) {
            self.pending = Some(candidate);
        } else {
            self.current = Some(candidate);
            self.pending = None;
        }
    }

    pub(super) fn visible_candidate(&self) -> Option<&Candidate> {
        self.current.as_ref().or(self.pending.as_ref())
    }
}

#[derive(Clone, Debug)]
pub(super) struct ProductEntry {
    pub(super) sources: BTreeMap<SourceId, SourceSlot>,
    pub(super) product_revision: ProductRevision,
    pub(super) published: Option<PublishedBody>,
}

impl ProductEntry {
    fn new() -> Self {
        Self {
            sources: BTreeMap::new(),
            product_revision: ProductRevision::new(0),
            published: None,
        }
    }

    fn snapshot(&self, product_id: WeatherProductId) -> Option<WeatherProductSnapshot> {
        self.published.as_ref().map(|published| {
            WeatherProductSnapshot::new(
                product_id,
                self.product_revision,
                published.source_product_revision,
                published.state.clone(),
                published.selection.clone(),
            )
        })
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct PublishedBody {
    pub(super) source_product_revision: SourceProductRevision,
    pub(super) state: crate::WeatherProductState,
    pub(super) selection: crate::SelectionExplanation,
}

fn classify_slot(
    product_id: &WeatherProductId,
    source: &SourceRef,
    revision: SourceProductRevision,
    event: &WeatherIngressEvent,
    slot: &SourceSlot,
) -> Result<AcceptStatus, StoreError> {
    if source.epoch > slot.epoch {
        return Ok(AcceptStatus::Applied);
    }
    if source.epoch < slot.epoch {
        return Ok(AcceptStatus::Stale);
    }
    let Some(latest) = slot.latest() else {
        return Ok(AcceptStatus::Applied);
    };
    if revision > latest.source_product_revision {
        return Ok(AcceptStatus::Applied);
    }
    if revision < latest.source_product_revision {
        return Ok(AcceptStatus::Stale);
    }
    if event == &latest.event {
        Ok(AcceptStatus::Replay)
    } else {
        Err(StoreError::ConflictingReplay {
            product_id: product_id.clone(),
            source_ref: source.clone(),
            revision,
        })
    }
}
