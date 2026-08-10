//! Product 12 pairing-bound tests.

#![allow(clippy::expect_used)]

use alloc::{format, vec};

use aero_link::{
    FisBContributor, FisBDeliveryPath, FisBOrigin, FisBProvenance, FisBSourceTimeQuality,
};
use airmass_core::{RawProductIdentity, WeatherAdvisoryType};

use super::{AdvisoryPairKey, AdvisoryPairing, PairedAdvisoryType};

#[test]
fn pairing_cache_evicts_at_its_fixed_bound() {
    let source = provenance(7);
    let contributor = source.contributors[0];
    let mut pairing = AdvisoryPairing::default();
    for report_number in 0_u16..65 {
        let key = AdvisoryPairKey::new(12, 26, report_number, contributor);
        let result = pairing.match_text(
            key,
            PairedAdvisoryType {
                value: WeatherAdvisoryType::Sigmet,
                issue_time: None,
                product_file_id: None,
                raw_identity: RawProductIdentity::new("test", format!("{report_number}")),
                source: source.clone(),
            },
        );
        assert!(result.is_ok());
    }
    let first = AdvisoryPairKey::new(12, 26, 0, contributor);
    let last = AdvisoryPairKey::new(12, 26, 64, contributor);
    assert!(pairing.advisory_type(first).is_none());
    assert!(pairing.advisory_type(last).is_some());
    assert_eq!(pairing.evictions(), 1);
}

#[test]
fn pair_key_separates_source_epochs() {
    let first_source = provenance(7);
    let second_source = provenance(8);
    let first = AdvisoryPairKey::new(12, 26, 1, first_source.contributors[0]);
    let second = AdvisoryPairKey::new(12, 26, 1, second_source.contributors[0]);
    assert_ne!(first, second);
}

fn provenance(source_epoch: u32) -> FisBProvenance {
    FisBProvenance {
        contributors: vec![FisBContributor {
            source_id: 7,
            source_epoch,
            origin: FisBOrigin::Replay,
            delivery_path: FisBDeliveryPath::Replay,
            first_received_at_micros: 1,
            last_received_at_micros: 1,
            station_identity: None,
            source_time_quality: FisBSourceTimeQuality::LocalIngress,
            uncertainty_micros: Some(0),
        }],
        contributors_overflowed: false,
    }
}
