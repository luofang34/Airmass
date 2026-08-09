//! Source-neutral meteorological state for aviation applications.
//!
//! This crate re-exports the portable [`airmass_core`] contract. Source
//! adapters are separate packages.

#![cfg_attr(not(feature = "std"), no_std)]

pub use airmass_core::*;
