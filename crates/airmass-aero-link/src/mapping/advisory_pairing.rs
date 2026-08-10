//! Holds bounded Product 12 text and graphical pairs.

use alloc::vec::Vec;

use aero_link::{FisBContributor, FisBExchangeProtocol, FisBProvenance, Product8Record};
use airmass_core::{RawProductIdentity, UtcTime, WeatherAdvisoryType};

use super::MappingError;

const MAX_PAIRED_REPORTS: usize = 64;

#[cfg(test)]
mod tests;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct AdvisoryPairKey {
    product_id: u16,
    report_year: u8,
    report_number: u16,
    source_id: u32,
    source_epoch: u32,
}

impl AdvisoryPairKey {
    pub(super) fn new(
        product_id: u16,
        report_year: u8,
        report_number: u16,
        contributor: FisBContributor,
    ) -> Self {
        Self {
            product_id,
            report_year,
            report_number,
            source_id: contributor.source_id,
            source_epoch: contributor.source_epoch,
        }
    }
}

#[derive(Debug)]
pub(super) struct PendingGraphicalAdvisory {
    pub(super) protocol: FisBExchangeProtocol,
    pub(super) product_id: u16,
    pub(super) records: Vec<Product8Record>,
    pub(super) source: FisBProvenance,
}

#[derive(Debug)]
pub(super) struct PairedAdvisoryType {
    pub(super) value: WeatherAdvisoryType,
    pub(super) issue_time: Option<UtcTime>,
    pub(super) product_file_id: Option<u16>,
    pub(super) raw_identity: RawProductIdentity,
    pub(super) source: FisBProvenance,
}

pub(super) struct MatchedAdvisoryPair<'a> {
    pub(super) pending: &'a PendingGraphicalAdvisory,
    pub(super) advisory_type: &'a PairedAdvisoryType,
}

#[derive(Debug)]
struct PairEntry {
    key: AdvisoryPairKey,
    advisory_type: Option<PairedAdvisoryType>,
    pending: Option<PendingGraphicalAdvisory>,
}

/// Bounded state for Product 12 text and graphical record pairing.
#[derive(Debug, Default)]
pub(crate) struct AdvisoryPairing {
    entries: Vec<PairEntry>,
    evictions: u64,
}

impl AdvisoryPairing {
    pub(super) fn advisory_type(&self, key: AdvisoryPairKey) -> Option<&PairedAdvisoryType> {
        self.entries
            .iter()
            .find(|entry| entry.key == key)
            .and_then(|entry| entry.advisory_type.as_ref())
    }

    pub(super) fn hold_graphical(
        &mut self,
        key: AdvisoryPairKey,
        pending: PendingGraphicalAdvisory,
    ) -> Result<(), MappingError> {
        if let Some(entry) = self.entries.iter_mut().find(|entry| entry.key == key) {
            entry.pending = Some(pending);
            return Ok(());
        }
        self.insert(PairEntry {
            key,
            advisory_type: None,
            pending: Some(pending),
        })
    }

    pub(super) fn match_text(
        &mut self,
        key: AdvisoryPairKey,
        advisory_type: PairedAdvisoryType,
    ) -> Result<bool, MappingError> {
        if let Some(entry) = self.entries.iter_mut().find(|entry| entry.key == key) {
            entry.advisory_type = Some(advisory_type);
            return Ok(entry.pending.is_some());
        }
        self.insert(PairEntry {
            key,
            advisory_type: Some(advisory_type),
            pending: None,
        })?;
        Ok(false)
    }

    pub(super) fn matched_pair(&self, key: AdvisoryPairKey) -> Option<MatchedAdvisoryPair<'_>> {
        let entry = self.entries.iter().find(|entry| entry.key == key)?;
        Some(MatchedAdvisoryPair {
            pending: entry.pending.as_ref()?,
            advisory_type: entry.advisory_type.as_ref()?,
        })
    }

    pub(super) fn clear_pending(&mut self, key: AdvisoryPairKey) {
        if let Some(entry) = self.entries.iter_mut().find(|entry| entry.key == key) {
            entry.pending = None;
        }
    }

    pub(super) fn cancel(&mut self, key: AdvisoryPairKey) {
        if let Some(index) = self.entries.iter().position(|entry| entry.key == key) {
            self.entries.remove(index);
        }
    }

    pub(crate) const fn evictions(&self) -> u64 {
        self.evictions
    }

    fn insert(&mut self, entry: PairEntry) -> Result<(), MappingError> {
        if self.entries.len() == MAX_PAIRED_REPORTS {
            self.entries.remove(0);
            self.evictions = self.evictions.wrapping_add(1);
        } else {
            self.entries
                .try_reserve_exact(1)
                .map_err(|source| MappingError::Allocation {
                    operation: "advisory pairing state",
                    source,
                })?;
        }
        self.entries.push(entry);
        Ok(())
    }
}
