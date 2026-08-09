//! Portable meteorological state types.
//!
//! Airmass uses the portable `aero-link` input contract. It does not enable
//! radio adapters.

#![cfg_attr(not(feature = "std"), no_std)]

mod fis_b;

pub use fis_b::accept_fis_b_product;
