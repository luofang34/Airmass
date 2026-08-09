//! Stateful FIS-B assembly at the source-adapter boundary.

use alloc::vec::Vec;

use aero_link::{
    FisBAssembler, FisBAssemblerConfig, FisBAssemblerConfigError, FisBAssemblerCounters,
    FisBAssemblyError, FisBContributor, FisBContributorError, FisBMessage,
    UatInformationFrameProjection, exchange_records_from_complete_fis_b_product,
};
use airmass_core::WeatherIngress;
use thiserror::Error;

use crate::{MappingError, mapping::map_records};

/// Stateful source adapter for one FIS-B assembly stream.
#[derive(Debug, Default)]
pub struct AeroLinkAdapter {
    assembler: FisBAssembler,
}

impl AeroLinkAdapter {
    /// Create an adapter with explicit assembly bounds.
    ///
    /// # Errors
    ///
    /// Returns [`FisBAssemblerConfigError`] when a bound is outside the
    /// protocol contract.
    pub fn new(config: FisBAssemblerConfig) -> Result<Self, FisBAssemblerConfigError> {
        Ok(Self {
            assembler: FisBAssembler::new(config)?,
        })
    }

    /// Accept one projected UAT information frame.
    ///
    /// A frame without a FIS-B APDU produces an empty batch.
    ///
    /// # Errors
    ///
    /// Returns [`AdapterError`] when provenance, assembly, or mapping fails.
    pub fn accept_projection(
        &mut self,
        projection: UatInformationFrameProjection<'_>,
    ) -> Result<WeatherIngressBatch, AdapterError> {
        let Some(message) = projection.frame.fis_b.as_ref() else {
            return Ok(WeatherIngressBatch::default());
        };
        let contributor = projection.fis_b_contributor()?;
        self.accept_message(message, contributor)
    }

    /// Accept one decoded APDU with its source context.
    ///
    /// # Errors
    ///
    /// Returns [`AdapterError`] when provenance, assembly, or mapping fails.
    pub fn accept_message(
        &mut self,
        message: &FisBMessage,
        contributor: FisBContributor,
    ) -> Result<WeatherIngressBatch, AdapterError> {
        let Some(product) = self
            .assembler
            .accept_with_contributor(message, contributor)?
        else {
            return Ok(WeatherIngressBatch::default());
        };
        let records =
            exchange_records_from_complete_fis_b_product(product).map_err(MappingError::from)?;
        Ok(map_records(records)?)
    }

    /// Get the bounded assembler counters.
    #[must_use]
    pub fn counters(&self) -> FisBAssemblerCounters {
        self.assembler.counters()
    }
}

/// Weather inputs emitted for one completed protocol record.
#[derive(Debug, Default)]
pub struct WeatherIngressBatch {
    pub(crate) values: Vec<WeatherIngress>,
    pub(crate) contributors_overflowed: bool,
}

impl WeatherIngressBatch {
    /// Borrow the emitted weather inputs.
    #[must_use]
    pub fn values(&self) -> &[WeatherIngress] {
        &self.values
    }

    /// Take the emitted weather inputs.
    #[must_use]
    pub fn into_values(self) -> Vec<WeatherIngress> {
        self.values
    }

    /// Test whether the source record exceeded its contributor bound.
    #[must_use]
    pub const fn contributors_overflowed(&self) -> bool {
        self.contributors_overflowed
    }
}

/// Failure while adapting one FIS-B APDU.
#[non_exhaustive]
#[derive(Debug, Error)]
pub enum AdapterError {
    /// The projected receiver context is outside the source contract.
    #[error("invalid FIS-B contributor context")]
    Contributor(#[from] FisBContributorError),
    /// Bounded product assembly rejected the APDU.
    #[error("FIS-B assembly failed")]
    Assembly(#[from] FisBAssemblyError),
    /// A completed protocol record could not map to weather ingress.
    #[error("FIS-B weather mapping failed")]
    Mapping(#[from] MappingError),
}
