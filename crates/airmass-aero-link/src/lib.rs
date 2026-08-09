//! Maps classified `aero-link` FIS-B records to Airmass weather ingress.

#![cfg_attr(not(feature = "std"), no_std)]

extern crate alloc;

mod adapter;
mod mapping;

pub use adapter::{AdapterError, AeroLinkAdapter, WeatherIngressBatch};
pub use mapping::MappingError;
