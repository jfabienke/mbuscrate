//! Decoding the value bytes of an M-Bus data record.
//!
//! The primitives a record parser needs before it can assemble anything: how long a
//! record's data field is, and how to read the integer and BCD encodings out of it.
//! Assembling an `MBusRecord` — with its units, quantities and vendor hooks — stays in
//! `mbus-rs` for now; these are the parts that are purely bytes-to-number.

use crate::error::ProtocolError;

/// Data-field length implied by a DIF's low nibble (EN 13757-3 §6.3.1).
///
/// `0x0D` means variable length — the next byte is an LVAR code, see
/// [`variable_data_length`]. `0x0F` and above are special functions with no data field.
pub fn dif_datalength_lookup(dif: u8) -> usize {
    match dif & 0x0F {
        0x00 => 0,
        0x01 => 1,
        0x02 => 2,
        0x03 => 3,
        0x04 => 4,
        0x05 => 4, // 32-bit real
        0x06 => 6,
        0x07 => 8,
        0x08 => 0, // selection for readout
        0x09 => 1, // 2-digit BCD
        0x0A => 2, // 4-digit BCD
        0x0B => 3, // 6-digit BCD
        0x0C => 4, // 8-digit BCD
        0x0D => 0, // variable length; the LVAR byte follows
        0x0E => 6, // 12-digit BCD
        _ => 0,    // 0x0F: special function
    }
}

/// Data length described by an LVAR byte (EN 13757-3 §6.4.3).
///
/// The `0xF0..=0xFA` range reaches 1130 bytes, far more than any single M-Bus frame can
/// carry — so a caller must still check the result against its own buffer *and* against
/// the remaining input. Returning the standard's value rather than clamping keeps that
/// decision where the buffer sizes are known.
pub fn variable_data_length(lvar: u8) -> Result<usize, ProtocolError> {
    if lvar <= 0xBF {
        Ok(lvar as usize)
    } else if (0xC0..=0xCF).contains(&lvar) {
        Ok((lvar - 0xC0) as usize * 2)
    } else if (0xD0..=0xDF).contains(&lvar) {
        Ok(((lvar - 0xD0) as usize * 2) + 1)
    } else if (0xE0..=0xEF).contains(&lvar) {
        Ok(((lvar - 0xE0) as usize) + 64)
    } else if (0xF0..=0xFA).contains(&lvar) {
        Ok(((lvar - 0xF0) as usize) + 1120)
    } else {
        Err(ProtocolError::InvalidField("unknown LVAR length code"))
    }
}

/// Little-endian signed integer from 1..=8 bytes, sign-extended.
pub fn int_le(data: &[u8]) -> i64 {
    let mut v: u64 = 0;
    for (i, &b) in data.iter().enumerate().take(8) {
        v |= (b as u64) << (8 * i);
    }
    let bits = 8 * data.len().min(8) as u32;
    if bits == 0 || bits == 64 {
        return v as i64;
    }
    // Sign-extend from the top bit of the field actually present.
    let sign = 1u64 << (bits - 1);
    if v & sign != 0 {
        (v | !((1u64 << bits) - 1)) as i64
    } else {
        v as i64
    }
}

/// Packed BCD, little-endian, with the sign in the top nibble of the last byte.
///
/// `0xF` in that nibble means negative (EN 13757-3 §6.3.3). Returns `None` if any nibble
/// is not a decimal digit — corrupt BCD must not decode to a plausible number.
///
/// Computed in integer arithmetic. The previous implementation built a decimal `String`
/// digit by digit and called `.parse::<f64>()`: an allocation, a formatting round trip and
/// a float parse to read a packed integer.
pub fn bcd_le(data: &[u8]) -> Option<f64> {
    if data.is_empty() {
        return None;
    }
    let (last, rest) = data.split_last()?;
    let negative = (last >> 4) == 0x0F;

    let mut magnitude: f64 = 0.0;
    // Most significant digits first, so each new digit shifts the accumulator by ten.
    if !negative {
        let hi = last >> 4;
        if hi > 9 {
            return None;
        }
        magnitude = hi as f64;
    }
    let lo = last & 0x0F;
    if lo > 9 {
        return None;
    }
    magnitude = magnitude * 10.0 + lo as f64;

    for b in rest.iter().rev() {
        for nib in [b >> 4, b & 0x0F] {
            if nib > 9 {
                return None;
            }
            magnitude = magnitude * 10.0 + nib as f64;
        }
    }
    Some(if negative { -magnitude } else { magnitude })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The previous String-based implementation, kept as a test oracle.
    ///
    /// Rewriting a decoder is exactly where a "simplification" silently changes results,
    /// so the new integer version is checked against the old one rather than against my
    /// expectations of it.
    fn bcd_le_via_string(data: &[u8]) -> Option<f64> {
        if data.is_empty() {
            return None;
        }
        let (last, rest) = data.split_last()?;
        let negative = (last >> 4) == 0x0F;
        let mut digits = String::new();
        if !negative {
            let hi = last >> 4;
            if hi > 9 {
                return None;
            }
            digits.push(char::from_digit(hi as u32, 10)?);
        }
        let lo = last & 0x0F;
        if lo > 9 {
            return None;
        }
        digits.push(char::from_digit(lo as u32, 10)?);
        for b in rest.iter().rev() {
            for nib in [b >> 4, b & 0x0F] {
                if nib > 9 {
                    return None;
                }
                digits.push(char::from_digit(nib as u32, 10)?);
            }
        }
        let magnitude: f64 = digits.parse().ok()?;
        Some(if negative { -magnitude } else { magnitude })
    }

    #[test]
    fn bcd_matches_the_string_implementation_it_replaced() {
        // Every 1- and 2-byte pattern, plus wider values including the sign nibble and
        // invalid digits.
        for a in 0u16..=255 {
            let one = [a as u8];
            assert_eq!(bcd_le(&one), bcd_le_via_string(&one), "1 byte {a:#04x}");
            for b in [0x00u8, 0x12, 0x99, 0x9A, 0xF0, 0xFF] {
                let two = [a as u8, b];
                assert_eq!(
                    bcd_le(&two),
                    bcd_le_via_string(&two),
                    "2 bytes {a:#04x} {b:#04x}"
                );
            }
        }
        for case in [
            &[0x34u8, 0x12][..],
            &[0x78, 0x56, 0x34, 0x12],
            &[0x78, 0x56, 0x34, 0xF2], // negative
            &[0x00, 0x00, 0x00, 0x00],
            &[0x99, 0x99, 0x99, 0x99],
            &[0x0A, 0x00], // invalid nibble
        ] {
            assert_eq!(bcd_le(case), bcd_le_via_string(case), "{case:02X?}");
        }
        assert_eq!(bcd_le(&[]), None);
    }

    #[test]
    fn bcd_reads_little_endian_with_a_sign_nibble() {
        assert_eq!(bcd_le(&[0x34, 0x12]), Some(1234.0));
        assert_eq!(bcd_le(&[0x34, 0xF2]), Some(-234.0));
        assert_eq!(bcd_le(&[0x0A]), None, "0xA is not a decimal digit");
    }

    /// The previous implementation, as an oracle for widths 1..=8.
    fn int_le_original(data: &[u8]) -> i64 {
        if data.is_empty() {
            return 0;
        }
        let mut v: i64 = 0;
        for (i, b) in data.iter().enumerate() {
            v |= (*b as i64) << (8 * i);
        }
        let bits = 8 * data.len() as u32;
        if bits < 64 && (v >> (bits - 1)) & 1 == 1 {
            v |= -1i64 << bits;
        }
        v
    }

    #[test]
    fn int_le_matches_the_implementation_it_replaced() {
        for len in 1..=8usize {
            for pattern in [0x00u8, 0x01, 0x7F, 0x80, 0xFF, 0xA5] {
                let data = vec![pattern; len];
                assert_eq!(
                    int_le(&data),
                    int_le_original(&data),
                    "len {len}, pattern {pattern:#04x}"
                );
            }
            // A high byte that drives the sign, with varied low bytes.
            let mut data = vec![0x12u8; len];
            data[len - 1] = 0x80;
            assert_eq!(int_le(&data), int_le_original(&data), "negative, len {len}");
        }
        assert_eq!(int_le(&[]), int_le_original(&[]));
    }

    #[test]
    fn int_le_truncates_rather_than_shifting_past_the_word() {
        // The original shifted by `8 * i` for every byte, so more than eight bytes
        // overflowed the shift — a debug panic. No DIF coding produces such a field, but
        // `int_le` takes a slice, so it must not depend on that.
        let long = [0xFFu8; 12];
        assert_eq!(int_le(&long), -1, "reads the low eight bytes, no panic");
    }

    #[test]
    fn int_le_sign_extends_from_the_field_width() {
        assert_eq!(int_le(&[0x01]), 1);
        assert_eq!(int_le(&[0xFF]), -1, "one byte, all bits set");
        assert_eq!(int_le(&[0xFF, 0xFF]), -1, "two bytes");
        assert_eq!(int_le(&[0x00, 0x80]), -32768, "16-bit minimum");
        assert_eq!(
            int_le(&[0xFF, 0x7F]),
            32767,
            "16-bit maximum stays positive"
        );
    }

    #[test]
    fn lvar_reaches_further_than_any_frame_can_carry() {
        // The caller must bound this: 1130 bytes cannot fit an M-Bus frame, let alone a
        // record buffer. Returning the standard's value keeps that check where the sizes
        // are known.
        assert_eq!(variable_data_length(0x00).unwrap(), 0);
        assert_eq!(variable_data_length(0xBF).unwrap(), 191);
        assert_eq!(variable_data_length(0xFA).unwrap(), 1130);
        assert!(variable_data_length(0xFB).is_err());
    }

    #[test]
    fn dif_lengths_cover_the_defined_codings() {
        assert_eq!(dif_datalength_lookup(0x00), 0);
        assert_eq!(dif_datalength_lookup(0x07), 8, "64-bit integer");
        assert_eq!(dif_datalength_lookup(0x0D), 0, "variable: LVAR follows");
        assert_eq!(dif_datalength_lookup(0x0E), 6, "12-digit BCD");
        assert_eq!(dif_datalength_lookup(0x0F), 0, "special function");
    }
}
