//! Decodes TWGO graphical advisory records.

use alloc::vec::Vec;

use airmass_core::{
    AdvisoryGeometry, AdvisoryLine, AdvisoryPolygon, AdvisoryPosition, AdvisoryRing,
    AdvisoryValidity, TimedField, WeatherAdvisoryType,
};

use super::{
    AdvisoryDecodeError, AdvisoryRecordInput, DecodedAdvisoryEvent, ResolvedIssueTime,
    advisory_with_type,
};
use crate::text_report::SourceEvidence;

#[cfg(test)]
mod tests;
mod validity;
mod vertical;

const ACTIVE_STATUS: u8 = 15;
const CANCELLED_STATUS: u8 = 13;
const AIRSPACE_OBJECT_TYPE: u8 = 14;
const VERTEX_BYTES: usize = 6;

#[derive(Clone, Copy)]
struct GraphicalHeader {
    overlay_record_id: u8,
    object_element: u8,
    object_status: u8,
    geometry_option: u8,
    vertex_count: usize,
    validity: AdvisoryValidity,
    vertex_offset: usize,
}

#[derive(Clone, Copy)]
struct ParsedRecord<'a> {
    input: AdvisoryRecordInput<'a>,
    header: GraphicalHeader,
}

#[derive(Clone, Copy)]
struct Vertex {
    position: AdvisoryPosition,
    altitude_feet: u32,
}

pub(super) fn decode(
    inputs: &[AdvisoryRecordInput<'_>],
    issue: Option<ResolvedIssueTime>,
    advisory_type: TimedField<WeatherAdvisoryType>,
    evidence: &SourceEvidence,
) -> Result<DecodedAdvisoryEvent, AdvisoryDecodeError> {
    let mut records = parse_records(inputs, issue)?;
    records.sort_by_key(|record| record.header.overlay_record_id);
    validate_record_set(&records)?;
    let Some(first) = records.first() else {
        return Err(AdvisoryDecodeError::EmptyGraphicalRecords);
    };
    if first.header.object_status == CANCELLED_STATUS {
        if first.input.product_id == 15 {
            return unsupported("object status", first.header.object_status);
        }
        return Ok(DecodedAdvisoryEvent::Cancellation {
            issue_time: issue.map(|value| value.utc),
        });
    }
    validate_active(first.header.object_status)?;
    active_event(&records, issue, advisory_type, evidence)
}

fn active_event(
    records: &[ParsedRecord<'_>],
    issue: Option<ResolvedIssueTime>,
    advisory_type: TimedField<WeatherAdvisoryType>,
    evidence: &SourceEvidence,
) -> Result<DecodedAdvisoryEvent, AdvisoryDecodeError> {
    let Some(first) = records.first() else {
        return Err(AdvisoryDecodeError::EmptyGraphicalRecords);
    };
    let vertices = joined_vertices(records)?;
    let reference = vertical::altitude_reference(first.header.geometry_option)?;
    let (altitude, band) =
        vertical::fields(&vertices, reference, first.header.geometry_option, evidence)?;
    let geometry = decode_geometry(first.header.geometry_option, &vertices)?;
    let input = first.input;
    let lifecycle = first.header.validity.lifecycle_interval()?;
    let identity = alloc::format!(
        "fisb-{}-{:02}-{}",
        input.product_id,
        input.metadata.report_year,
        input.metadata.report_number
    );
    Ok(DecodedAdvisoryEvent::Product {
        advisory: advisory_with_type(
            identity,
            advisory_type,
            first.header.validity,
            geometry,
            altitude,
            band,
            evidence,
        ),
        issue_time: issue.map(|value| value.utc),
        valid_period: lifecycle,
    })
}

fn parse_records<'a>(
    inputs: &[AdvisoryRecordInput<'a>],
    issue: Option<ResolvedIssueTime>,
) -> Result<Vec<ParsedRecord<'a>>, AdvisoryDecodeError> {
    let mut records = Vec::new();
    reserve(&mut records, inputs.len(), "graphical record set")?;
    for input in inputs {
        records.push(ParsedRecord {
            input: *input,
            header: parse_header(
                input.payload,
                input.metadata.report_year,
                input.product_id,
                issue,
            )?,
        });
    }
    Ok(records)
}

fn validate_record_set(records: &[ParsedRecord<'_>]) -> Result<(), AdvisoryDecodeError> {
    let Some(first) = records.first() else {
        return Err(AdvisoryDecodeError::EmptyGraphicalRecords);
    };
    if records.len() > 15 {
        return Err(AdvisoryDecodeError::GraphicalRecordCount {
            count: records.len(),
        });
    }
    for (expected, record) in records.iter().enumerate() {
        let expected =
            u8::try_from(expected).map_err(|_| AdvisoryDecodeError::GraphicalRecordCount {
                count: records.len(),
            })?;
        validate_record_identity(first, record)?;
        validate_record_header(first, record)?;
        if record.header.overlay_record_id != expected {
            return Err(AdvisoryDecodeError::GraphicalRecordSequence {
                expected,
                found: record.header.overlay_record_id,
            });
        }
    }
    Ok(())
}

fn validate_record_identity(
    first: &ParsedRecord<'_>,
    record: &ParsedRecord<'_>,
) -> Result<(), AdvisoryDecodeError> {
    if record.input.product_id == first.input.product_id
        && record.input.metadata.report_year == first.input.metadata.report_year
        && record.input.metadata.report_number == first.input.metadata.report_number
    {
        Ok(())
    } else {
        Err(AdvisoryDecodeError::InconsistentGraphicalField {
            field: "report identity",
        })
    }
}

fn validate_record_header(
    first: &ParsedRecord<'_>,
    record: &ParsedRecord<'_>,
) -> Result<(), AdvisoryDecodeError> {
    if record.header.object_status == first.header.object_status
        && record.header.object_element == first.header.object_element
        && record.header.geometry_option == first.header.geometry_option
        && record.header.validity == first.header.validity
    {
        Ok(())
    } else {
        Err(AdvisoryDecodeError::InconsistentGraphicalField {
            field: "record header",
        })
    }
}

fn validate_active(status: u8) -> Result<(), AdvisoryDecodeError> {
    if status == ACTIVE_STATUS {
        Ok(())
    } else {
        unsupported("object status", status)
    }
}

fn joined_vertices(records: &[ParsedRecord<'_>]) -> Result<Vec<Vertex>, AdvisoryDecodeError> {
    let total = records.iter().try_fold(0_usize, |sum, record| {
        sum.checked_add(record.header.vertex_count)
    });
    let Some(total) = total else {
        return Err(AdvisoryDecodeError::GraphicalVertexCountOverflow);
    };
    let mut vertices = Vec::new();
    reserve(&mut vertices, total, "joined graphical vertices")?;
    for record in records {
        let mut fragment = decode_vertices(record.input.payload, &record.header)?;
        vertices.append(&mut fragment);
    }
    Ok(vertices)
}

fn parse_header(
    payload: &[u8],
    report_year: u8,
    product_id: u16,
    issue: Option<ResolvedIssueTime>,
) -> Result<GraphicalHeader, AdvisoryDecodeError> {
    let fixed = bytes_at(payload, 0, 5, "graphical header")?;
    let label_flag = u8::from(fixed[4] & 1 != 0);
    if matches!(product_id, 14 | 15) && label_flag != 0 {
        return unsupported("object label flag", label_flag);
    }
    let label_bytes = if label_flag == 0 { 2 } else { 9 };
    let object_offset = 5 + label_bytes;
    let object = bytes_at(payload, object_offset, 2, "object fields")?;
    validate_object_fields(object, product_id)?;
    let qualifier_bytes = if object[0] & 0x40 == 0 { 0 } else { 3 };
    let timing_offset = object_offset + 2 + qualifier_bytes;
    let timing = bytes_at(payload, timing_offset, 2, "timing fields")?;
    validate_timing_fields(timing, product_id)?;
    let validity_offset = timing_offset + 2;
    let validity_bytes = bytes_at(payload, validity_offset, 8, "record applicability")?;
    Ok(GraphicalHeader {
        overlay_record_id: (fixed[4] >> 1) & 0x0f,
        object_element: object[0] & 0x1f,
        object_status: object[1] & 0x0f,
        geometry_option: timing[0] & 0x0f,
        vertex_count: usize::from(timing[1] & 0x3f) + 1,
        validity: validity::decode(report_year, validity_bytes, product_id, issue)?,
        vertex_offset: validity_offset + validity_bytes.len(),
    })
}

fn validate_object_fields(object: &[u8], product_id: u16) -> Result<(), AdvisoryDecodeError> {
    let element_flag = u8::from(object[0] & 0x80 != 0);
    let expected_element_flag = u8::from(product_id == 14);
    if element_flag != expected_element_flag {
        return unsupported("object element flag", element_flag);
    }
    if object[0] & 0x20 != 0 {
        return unsupported("object parameter flag", 1);
    }
    let object_element = object[0] & 0x1f;
    if product_id == 14 && !(1..=7).contains(&object_element) {
        return unsupported("object element", object_element);
    }
    let qualifier_flag = u8::from(object[0] & 0x40 != 0);
    if qualifier_flag != 0 && (product_id != 14 || !matches!(object_element, 6 | 7)) {
        return unsupported("object qualifier flag", qualifier_flag);
    }
    let object_type = object[1] >> 4;
    if object_type != AIRSPACE_OBJECT_TYPE {
        return unsupported("object type", object_type);
    }
    Ok(())
}

fn validate_timing_fields(timing: &[u8], product_id: u16) -> Result<(), AdvisoryDecodeError> {
    let applicability = timing[0] >> 6;
    if applicability != 3 {
        return unsupported("record applicability options", applicability);
    }
    let format = (timing[0] >> 4) & 0x03;
    if format != 1 {
        return unsupported("date/time format", format);
    }
    let geometry = timing[0] & 0x0f;
    if !matches!(geometry, 3 | 4 | 11 | 12) {
        return unsupported("overlay geometry option", geometry);
    }
    if product_id == 15 && !matches!(geometry, 3 | 4) {
        return unsupported("overlay geometry option", geometry);
    }
    let operator = timing[1] >> 6;
    if operator != 0 {
        return unsupported("overlay operator", operator);
    }
    Ok(())
}

fn decode_vertices(
    payload: &[u8],
    header: &GraphicalHeader,
) -> Result<Vec<Vertex>, AdvisoryDecodeError> {
    let required = header.vertex_count * VERTEX_BYTES;
    let bytes = bytes_at(payload, header.vertex_offset, required, "overlay vertices")?;
    let trailing = payload
        .len()
        .saturating_sub(header.vertex_offset + required);
    if trailing != 0 {
        return Err(AdvisoryDecodeError::TrailingGraphicalBytes {
            remaining: trailing,
        });
    }
    let mut vertices = Vec::new();
    reserve(&mut vertices, header.vertex_count, "graphical vertices")?;
    for encoded in bytes.chunks_exact(VERTEX_BYTES) {
        vertices.push(decode_vertex(encoded)?);
    }
    Ok(vertices)
}

fn decode_vertex(bytes: &[u8]) -> Result<Vertex, AdvisoryDecodeError> {
    let encoded = bytes
        .iter()
        .fold(0_u64, |value, byte| (value << 8) | u64::from(*byte));
    let longitude = ((encoded >> 29) & 0x7ffff) as u32;
    let latitude = ((encoded >> 10) & 0x7ffff) as u32;
    let altitude = (encoded & 0x3ff) as u32;
    Ok(Vertex {
        position: AdvisoryPosition::new(
            angular_microdegrees(latitude),
            angular_microdegrees(longitude),
        )?,
        altitude_feet: altitude * 100,
    })
}

fn angular_microdegrees(encoded: u32) -> i32 {
    let signed = if encoded & 0x40000 == 0 {
        i64::from(encoded)
    } else {
        i64::from(encoded) - 0x80000
    };
    let numerator = signed * 360_000_000;
    let rounded = if numerator < 0 {
        numerator - 262_144
    } else {
        numerator + 262_144
    };
    (rounded / 524_288) as i32
}

fn decode_geometry(
    option: u8,
    vertices: &[Vertex],
) -> Result<AdvisoryGeometry, AdvisoryDecodeError> {
    match option {
        3 | 4 => polygon(vertices).map(AdvisoryGeometry::Polygon),
        11 | 12 => line(vertices).map(AdvisoryGeometry::Polyline),
        _ => unsupported("overlay geometry option", option),
    }
}

fn polygon(vertices: &[Vertex]) -> Result<AdvisoryPolygon, AdvisoryDecodeError> {
    let mut rings = Vec::new();
    reserve(&mut rings, 1, "advisory rings")?;
    let mut positions = Vec::new();
    reserve(&mut positions, vertices.len(), "advisory ring positions")?;
    for vertex in vertices {
        positions.push(vertex.position);
        if positions.len() >= 4 && positions.first() == positions.last() {
            append_distinct_ring(&mut rings, core::mem::take(&mut positions))?;
        }
    }
    if !positions.is_empty() {
        if positions.len() < 3 {
            return Err(AdvisoryDecodeError::InsufficientPositions {
                count: positions.len(),
                required: 3,
            });
        }
        positions.push(positions[0]);
        append_distinct_ring(&mut rings, positions)?;
    }
    AdvisoryPolygon::new(rings).map_err(AdvisoryDecodeError::Geometry)
}

fn append_distinct_ring(
    rings: &mut Vec<AdvisoryRing>,
    positions: Vec<AdvisoryPosition>,
) -> Result<(), AdvisoryDecodeError> {
    let ring = AdvisoryRing::new(positions)?;
    if !rings
        .iter()
        .any(|current| current.positions() == ring.positions())
    {
        rings
            .try_reserve(1)
            .map_err(|source| AdvisoryDecodeError::Allocation {
                operation: "advisory rings",
                source,
            })?;
        rings.push(ring);
    }
    Ok(())
}

fn line(vertices: &[Vertex]) -> Result<AdvisoryLine, AdvisoryDecodeError> {
    let mut positions = Vec::new();
    reserve(&mut positions, vertices.len(), "advisory line positions")?;
    positions.extend(vertices.iter().map(|vertex| vertex.position));
    AdvisoryLine::new(positions).map_err(AdvisoryDecodeError::Geometry)
}

fn bytes_at<'a>(
    bytes: &'a [u8],
    offset: usize,
    required: usize,
    field: &'static str,
) -> Result<&'a [u8], AdvisoryDecodeError> {
    bytes
        .get(offset..offset.saturating_add(required))
        .ok_or(AdvisoryDecodeError::Truncated {
            field,
            required,
            remaining: bytes.len().saturating_sub(offset),
        })
}

fn unsupported<T>(field: &'static str, value: u8) -> Result<T, AdvisoryDecodeError> {
    Err(AdvisoryDecodeError::UnsupportedGraphicalField { field, value })
}

fn reserve<T>(
    values: &mut Vec<T>,
    count: usize,
    operation: &'static str,
) -> Result<(), AdvisoryDecodeError> {
    values
        .try_reserve_exact(count)
        .map_err(|source| AdvisoryDecodeError::Allocation { operation, source })
}
