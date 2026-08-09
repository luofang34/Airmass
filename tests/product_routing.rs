//! Public contract tests for complete FIS-B product admission.

#![allow(clippy::panic)]

use aero_link::{
    CompleteFisBProduct, FisBAssembler, FisBOwner, decode_fis_b_apdu, route_fis_b_product,
};
use airmass::accept_fis_b_product;

const UNAVAILABLE_REPORT_DLAC: [u8; 43] = [
    0x18, 0x94, 0xed, 0x0a, 0x0c, 0xb2, 0xc3, 0x06, 0xa0, 0x68, 0xd0, 0x6c, 0x81, 0xa2, 0x98, 0x80,
    0x7b, 0x41, 0x25, 0x23, 0x45, 0x52, 0x04, 0x12, 0x3c, 0x45, 0x43, 0x52, 0x05, 0x50, 0x10, 0x15,
    0x05, 0x4e, 0x05, 0x4e, 0x05, 0x60, 0x49, 0x30, 0x10, 0x8c, 0x14,
];
const NOTICE_DLAC: [u8; 13] = [
    0x2d, 0x30, 0xcb, 0x81, 0x25, 0xd9, 0x83, 0x2e, 0x52, 0x80, 0x33, 0x13, 0x10,
];

#[test]
fn mixed_products_keep_only_the_airmass_assignments() {
    let products = [
        complete_product(13, &[]),
        complete_product(413, &[0x50, 0x11, 0xa0]),
        complete_product(351, &[]),
        complete_product(63, &[0xca, 0xfe]),
        complete_product(700, &[0xde, 0xad]),
    ];

    let kept: Vec<_> = products
        .into_iter()
        .filter_map(accept_fis_b_product)
        .collect();
    let identifiers: Vec<_> = kept.iter().map(|product| product.product.id).collect();

    assert_eq!(identifiers, [413, 63]);
    assert_eq!(kept[1].payload, [0xca, 0xfe]);
}

#[test]
fn product_eight_uses_the_payload_sensitive_upstream_route() {
    let notice = complete_product(8, &NOTICE_DLAC);
    let unavailable = complete_product(8, &UNAVAILABLE_REPORT_DLAC);

    assert_eq!(route_fis_b_product(&notice), FisBOwner::Airspace);
    assert_eq!(route_fis_b_product(&unavailable), FisBOwner::Diagnostics);
    assert!(accept_fis_b_product(notice).is_none());
    assert!(accept_fis_b_product(unavailable).is_none());
}

fn complete_product(product_id: u16, payload: &[u8]) -> CompleteFisBProduct {
    let mut apdu = vec![
        ((product_id >> 6) & 0x1f) as u8,
        ((product_id & 0x3f) << 2) as u8,
        0,
        0,
    ];
    apdu.extend_from_slice(payload);
    let message = decode_fis_b_apdu(&apdu);
    let Ok(message) = message else {
        panic!("the product fixture must decode");
    };
    let product = FisBAssembler::default().accept(&message, 0);
    let Some(product) = product else {
        panic!("an unsegmented product must complete at once");
    };
    product
}
