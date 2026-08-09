//! Admission of complete FIS-B products.

use aero_link::{CompleteFisBProduct, FisBOwner, route_fis_b_product};

/// Keep a complete product when the routing table assigns it to Airmass.
///
/// This function returns `None` for products that belong to Airspace or
/// Diagnostics. It does not assemble segments or hold product state.
#[must_use]
pub fn accept_fis_b_product(product: CompleteFisBProduct) -> Option<CompleteFisBProduct> {
    if route_fis_b_product(&product) == FisBOwner::Airmass {
        Some(product)
    } else {
        None
    }
}
