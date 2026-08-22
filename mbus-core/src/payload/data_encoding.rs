//! M-Bus value encodings: BCD, integers, floats, dates and manufacturer codes.
//!
//! All of these are functions of bytes. What is deliberately *not* here is any conversion
//! to a wall-clock type: [`decode_date_time`] yields an [`MBusDateTime`] with the fields
//! the wire actually carries, and turning that into a `SystemTime` is the caller's job —
//! `mbus-rs` does it. A core that reached for `UNIX_EPOCH` would need a clock, and on a
//! microcontroller there may not be one.

use crate::error::ProtocolError;
use nom::{bytes::complete::take, IResult, Parser};

/// A date and time exactly as an M-Bus telegram carries it.
///
/// Fields are the wire values, already validated: `month` is 1..=12 and `day` 1..=31. No
/// epoch, no timezone, no leap-second policy — those are decisions for whoever turns this
/// into an instant.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub struct MBusDateTime {
    /// Years since 2000, as the wire encodes it.
    pub year: u16,
    /// 1..=12.
    pub month: u8,
    /// 1..=31.
    pub day: u8,
    pub hour: u8,
    pub minute: u8,
    pub second: u8,
}

/// Why a date could not be decoded.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum DateTimeError {
    /// Length is not one of the defined compound types (2, 4 or 6 bytes).
    UnsupportedLength(usize),
    /// The "invalid" bit is set — the meter is telling us its clock is not set.
    MarkedInvalid,
    /// A field is outside the range the standard permits, e.g. month 0.
    FieldOutOfRange,
}

impl core::fmt::Display for DateTimeError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::UnsupportedLength(n) => write!(f, "unsupported date length {n} (want 2, 4 or 6)"),
            Self::MarkedInvalid => write!(f, "meter marked the time invalid"),
            Self::FieldOutOfRange => write!(f, "date field out of range"),
        }
    }
}

impl core::error::Error for DateTimeError {}

/// Month index 1..=12 from the low nibble of a date byte, or `None`.
///
/// EN 13757-3 stores the month as 1..=12. A meter whose clock has never been set sends 0,
/// which a bare `- 1` underflowed: a panic in debug, and in release a wrap to 255 that
/// silently produced a date about 21 years in the future.
fn month_of(byte: u8) -> Option<u8> {
    match byte & 0x0F {
        m @ 1..=12 => Some(m),
        _ => None,
    }
}

/// Decode a Type G (2), Type F (4) or Type I (6) compound date/time.
pub fn decode_date_time(input: &[u8]) -> Result<MBusDateTime, DateTimeError> {
    match input.len() {
        // Type G: CP16, date only.
        2 => Ok(MBusDateTime {
            year: u16::from(((input[0] & 0xE0) >> 5) | ((input[1] & 0xF0) >> 1)),
            month: month_of(input[1]).ok_or(DateTimeError::FieldOutOfRange)?,
            day: input[0] & 0x1F,
            ..Default::default()
        }),
        // Type F: CP32, date and time. Bit 7 of byte 0 marks the value invalid.
        4 => {
            if input[0] & 0x80 != 0 {
                return Err(DateTimeError::MarkedInvalid);
            }
            Ok(MBusDateTime {
                year: u16::from(((input[2] & 0xE0) >> 5) | ((input[3] & 0xF0) >> 1)),
                month: month_of(input[3]).ok_or(DateTimeError::FieldOutOfRange)?,
                day: input[2] & 0x1F,
                hour: input[1] & 0x1F,
                minute: input[0] & 0x3F,
                ..Default::default()
            })
        }
        // Type I: CP48, date and time with seconds.
        6 => Ok(MBusDateTime {
            year: u16::from(((input[3] & 0xE0) >> 5) | ((input[4] & 0xF0) >> 1)),
            month: month_of(input[4]).ok_or(DateTimeError::FieldOutOfRange)?,
            day: input[3] & 0x1F,
            hour: input[2] & 0x1F,
            minute: input[1] & 0x3F,
            second: input[0] & 0x3F,
        }),
        n => Err(DateTimeError::UnsupportedLength(n)),
    }
}

/// Decode a BCD value to a 32-bit unsigned integer.
///
/// Rejects nibbles above 9: BCD digits are 0..=9, and a byte outside that is not a
/// short read but corrupt data, so it must not be folded into a plausible number.
pub fn decode_bcd(input: &[u8]) -> IResult<&[u8], u32> {
    let (input, bytes) = take(4usize).parse(input)?;

    for byte in bytes {
        if (byte & 0xF) > 9 || ((byte >> 4) & 0xF) > 9 {
            return Err(nom::Err::Error(nom::error::Error::new(
                input,
                nom::error::ErrorKind::Verify,
            )));
        }
    }

    let mut value = 0u32;
    let mut multiplier = 1u32;
    for &byte in bytes.iter().rev() {
        // Low nibble is the ones digit, high nibble the tens.
        value += (byte as u32 & 0xF) * multiplier;
        multiplier *= 10;
        value += ((byte >> 4) as u32 & 0xF) * multiplier;
        multiplier *= 10;
    }

    Ok((input, value))
}

/// Encode a `u32` as four BCD bytes.
///
/// Four bytes hold eight decimal digits, so values above 99_999_999 lose their high
/// digits — the same behaviour as before, when this returned a `vec![0u8; 4]`.
pub fn encode_bcd(mut input: u32) -> [u8; 4] {
    let mut result = [0u8; 4];
    for idx in (0..4).rev() {
        if input > 0 {
            let ones = (input % 10) as u8;
            input /= 10;
            let tens = (input % 10) as u8;
            input /= 10;
            result[idx] = (tens << 4) | ones;
        }
    }
    result
}

/// Manufacturer code as its three letters.
///
/// Returns a fixed-capacity string: the code is always exactly three characters, packed
/// five bits each into a 16-bit word (EN 13757-3 §5.6).
pub fn decode_manufacturer(byte1: u8, byte2: u8) -> heapless::String<3> {
    let mut id = ((byte1 as u32) << 8) + (byte2 as u32);
    let mut out = heapless::String::new();
    // Exactly three pushes into a capacity-3 string; the Results cannot be Err.
    let _ = out.push(char::from_u32((id / (32 * 32)) + 64).unwrap_or('?'));
    id %= 32 * 32;
    let _ = out.push(char::from_u32((id / 32) + 64).unwrap_or('?'));
    id %= 32;
    let _ = out.push(char::from_u32(id + 64).unwrap_or('?'));
    out
}

/// Pack three letters into the two-byte manufacturer code.
pub fn encode_manufacturer(manufacturer: &str) -> Result<[u8; 2], ProtocolError> {
    let b = manufacturer.as_bytes();
    if b.len() != 3 || !b.iter().all(|c| c.is_ascii_alphabetic()) {
        return Err(ProtocolError::InvalidField(
            "manufacturer must be three ASCII letters",
        ));
    }
    // Uppercase, then five bits each. `as_bytes` rather than `chars().next().unwrap()`,
    // which is what the previous version used — provably safe, but only provably.
    let v = (u32::from(b[0].to_ascii_uppercase() - 64) * 32 * 32)
        + (u32::from(b[1].to_ascii_uppercase() - 64) * 32)
        + u32::from(b[2].to_ascii_uppercase() - 64);
    Ok([(v >> 8) as u8, (v & 0xFF) as u8])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_zero_month_is_out_of_range_not_an_underflow() {
        // The bug this replaced: `(byte & 0x0F) - 1` on an unset meter clock.
        assert_eq!(
            decode_date_time(&[0x01, 0x00]),
            Err(DateTimeError::FieldOutOfRange)
        );
        assert_eq!(
            decode_date_time(&[0x00, 0x0D]),
            Err(DateTimeError::FieldOutOfRange),
            "month 13 is out of range too"
        );
    }

    #[test]
    fn type_g_carries_date_only() {
        // day 1, month 1, year bits clear
        let d = decode_date_time(&[0x01, 0x01]).unwrap();
        assert_eq!((d.day, d.month), (1, 1));
        assert_eq!((d.hour, d.minute, d.second), (0, 0, 0));
    }

    #[test]
    fn type_f_honours_the_invalid_bit() {
        // Bit 7 of byte 0 set: the meter is saying its clock is not set.
        assert_eq!(
            decode_date_time(&[0x80, 0x00, 0x01, 0x01]),
            Err(DateTimeError::MarkedInvalid)
        );
    }

    #[test]
    fn type_i_carries_seconds() {
        let d = decode_date_time(&[0x3B, 0x3A, 0x17, 0x01, 0x01, 0x00]).unwrap();
        assert_eq!(d.second, 59);
        assert_eq!(d.minute, 58);
        assert_eq!(d.hour, 23);
    }

    #[test]
    fn unsupported_lengths_report_what_they_got() {
        assert_eq!(
            decode_date_time(&[0u8; 3]),
            Err(DateTimeError::UnsupportedLength(3))
        );
    }

    #[test]
    fn bcd_round_trips_through_four_bytes() {
        for v in [0u32, 1, 42, 12_345_678] {
            let (_, back) = decode_bcd(&encode_bcd(v)).unwrap();
            assert_eq!(back, v, "BCD round trip for {v}");
        }
    }

    #[test]
    fn manufacturer_round_trips() {
        for code in ["KAM", "QDS", "ABC"] {
            let packed = encode_manufacturer(code).unwrap();
            assert_eq!(decode_manufacturer(packed[0], packed[1]).as_str(), code);
        }
        assert!(encode_manufacturer("AB").is_err(), "too short");
        assert!(encode_manufacturer("AB1").is_err(), "not all letters");
    }
}
