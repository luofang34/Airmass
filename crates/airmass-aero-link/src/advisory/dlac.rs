//! Decodes Data Link ASCII Characters text.

use alloc::string::String;

use super::AdvisoryDecodeError;

const ALPHABET: &[u8; 64] =
    b"\x03ABCDEFGHIJKLMNOPQRSTUVWXYZ\x1a\x09\x1e\x0a| !\"#$%&'()*+,-./0123456789:;<=>?";

pub(super) fn decode(payload: &[u8]) -> Result<String, AdvisoryDecodeError> {
    let mut output = String::new();
    output
        .try_reserve(payload.len().saturating_mul(2))
        .map_err(|source| AdvisoryDecodeError::Allocation {
            operation: "DLAC advisory text",
            source,
        })?;
    let mut accumulator = 0_u32;
    let mut available_bits = 0_u8;
    let mut pending_spaces = false;
    for byte in payload {
        accumulator = (accumulator << 8) | u32::from(*byte);
        available_bits = available_bits.wrapping_add(8);
        while available_bits >= 6 {
            available_bits -= 6;
            let symbol = ((accumulator >> available_bits) & 0x3f) as u8;
            append_symbol(&mut output, symbol, &mut pending_spaces)?;
            accumulator = if available_bits == 0 {
                0
            } else {
                accumulator & ((1_u32 << available_bits) - 1)
            };
        }
    }
    Ok(output)
}

fn append_symbol(
    output: &mut String,
    symbol: u8,
    pending_spaces: &mut bool,
) -> Result<(), AdvisoryDecodeError> {
    if *pending_spaces {
        output.try_reserve(usize::from(symbol)).map_err(|source| {
            AdvisoryDecodeError::Allocation {
                operation: "DLAC advisory spaces",
                source,
            }
        })?;
        for _ in 0..symbol {
            output.push(' ');
        }
        *pending_spaces = false;
    } else if symbol == 28 {
        *pending_spaces = true;
    } else {
        output.push(char::from(ALPHABET[usize::from(symbol)]));
    }
    Ok(())
}
