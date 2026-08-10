//! Graphical advisory record-limit tests.

use alloc::vec::Vec;

use airmass_core::{AdvisoryValidity, UtcTime};

use super::{
    AdvisoryDecodeError, AdvisoryRecordInput, GraphicalHeader, ParsedRecord, validate_record_set,
};
use crate::advisory::AdvisoryRecordMetadata;

#[test]
fn graphical_report_rejects_sixteen_records() {
    let validity = AdvisoryValidity::Point(UtcTime::from_unix_millis(1_000));
    let mut records = Vec::new();
    for overlay_record_id in 0_u8..16 {
        records.push(ParsedRecord {
            input: AdvisoryRecordInput {
                product_id: 14,
                payload: &[],
                decoded_text: None,
                metadata: AdvisoryRecordMetadata {
                    record_format: 8,
                    report_number: 1,
                    report_year: 26,
                    report_status: None,
                },
            },
            header: GraphicalHeader {
                overlay_record_id,
                object_element: 0,
                object_status: 15,
                geometry_option: 3,
                vertex_count: 1,
                validity,
                vertex_offset: 0,
            },
        });
    }
    assert!(matches!(
        validate_record_set(&records),
        Err(AdvisoryDecodeError::GraphicalRecordCount { count: 16 })
    ));
}
