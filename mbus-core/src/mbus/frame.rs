//! # M-Bus Protocol Decoder
//!
//! This module provides functionality to decode and encode Meter-Bus (M-Bus) protocol frames,
//! commonly used for reading data from utility meters (e.g., electricity, gas, water).
//! It leverages the `nom` crate for efficient and reliable parsing of binary data.
//!
//! ## Features
//! - Parse and pack different types of M-Bus frames: Acknowledgment, Short, Control, and Long frames.
//! - Verify frame integrity through checksum validation.
//! - Detailed error handling for robust parsing and data integrity checks.
//!
//! ## Usage
//!
//! Parsing an M-Bus frame from a byte slice:
//! ```ignore
//! let bytes: &[u8] = &[
//!     // Example byte slice for an M-Bus frame
//! ];
//! let parsed_frame = parse_frame(bytes);
//! match parsed_frame {
//!     Ok((remaining, frame)) => {
//!         // Handle successfully parsed frame
//!     },
//!     Err(error) => {
//!         // Handle parsing error
//!     }
//! }
//! ```
//!
//! Packing an M-Bus frame into a byte vector:
//! ```ignore
//! let frame = MBusFrame {
//!     frame_type: MBusFrameType::Short,
//!     control: 0x7B,
//!     address: 0x10,
//!     control_information: 0x51,
//!     data: FrameData::new(),
//!     checksum: 0x1F,
//! };
//! let bytes = pack_frame(&frame);
//! // `bytes` now contains the binary representation of the M-Bus frame
//! ```
//!
//! Verifying the checksum of an M-Bus frame:
//! ```ignore
//! let verification_result = verify_frame(&frame);
//! match verification_result {
//!     Ok(()) => {
//!         // Frame is valid
//!     },
//!     Err(error) => {
//!         // Handle invalid frame, e.g., checksum mismatch
//!     }
//! }
//! ```
//!
//! ## Error Handling
//! This module uses custom errors (defined in `MBusError`) to indicate various failure states,
//! such as parsing errors or checksum mismatches. This allows for precise error handling
//! and robust applications.
//!
//! Note: Replace example byte slices and frame values with actual data as needed.

use crate::constants::{
    MBUS_ADDRESS_NETWORK_LAYER, MBUS_CONTROL_INFO_SELECT_SLAVE, MBUS_CONTROL_MASK_FCB,
    MBUS_CONTROL_MASK_SND_UD,
};
use crate::error::ProtocolError;
use nom::{bytes::complete::take_while_m_n, number::complete::be_u8, Err as NomErr, IResult};

/// Largest M-Bus data payload: the length field counts C + A + CI + data and is one byte,
/// so 255 - 3. A protocol bound, not a chosen buffer size.
pub const MBUS_MAX_DATA: usize = 252;

/// Largest packed frame: `68 L L 68` + L bytes + checksum + `16`, i.e. 6 + 255.
pub const MBUS_MAX_FRAME: usize = 261;

/// An M-Bus frame payload.
pub type FrameData = heapless::Vec<u8, MBUS_MAX_DATA>;
/// A packed frame, ready for the wire.
pub type PackedFrame = heapless::Vec<u8, MBUS_MAX_FRAME>;

/// Represents an M-Bus frame.
#[derive(Debug, PartialEq, Eq)]
pub struct MBusFrame {
    pub frame_type: MBusFrameType,
    pub control: u8,
    pub address: u8,
    pub control_information: u8,
    pub data: FrameData,
    pub checksum: u8,
    pub more_records_follow: bool,
}

/// Represents the different types of M-Bus frames.
#[derive(Debug, PartialEq, Eq, Clone)]
pub enum MBusFrameType {
    Ack,
    Short,
    Control,
    Long,
}

/// Uses the `nom` crate to parse an M-Bus frame from a byte slice.
pub fn parse_frame(input: &[u8]) -> IResult<&[u8], MBusFrame> {
    let (mut input, (frame_type, len1_opt)) = parse_frame_type(input)?;

    match frame_type {
        MBusFrameType::Ack => Ok((
            input,
            MBusFrame {
                frame_type,
                control: 0,
                address: 0,
                control_information: 0,
                data: FrameData::new(),
                checksum: 0,
                more_records_follow: false,
            },
        )),
        MBusFrameType::Short => {
            let (i, control) = be_u8(input)?;
            let (i, address) = be_u8(i)?;
            let (i, (_ci, data, checksum)) = parse_short_frame(i)?;
            input = i;
            Ok((
                input,
                MBusFrame {
                    frame_type,
                    control,
                    address,
                    control_information: 0,
                    data,
                    checksum,
                    more_records_follow: false,
                },
            ))
        }
        MBusFrameType::Control | MBusFrameType::Long => {
            let (i, start2) = be_u8(input)?;
            if start2 != 0x68 {
                return Err(NomErr::Error(nom::error::Error::new(
                    i,
                    nom::error::ErrorKind::Tag,
                )));
            }
            let (i, control) = be_u8(i)?;
            let (i, address) = be_u8(i)?;
            let len1 = len1_opt.unwrap_or(3) as usize;
            let (i, (control_information, data, checksum)) =
                parse_control_or_long_frame_after_header(i, len1)?;
            input = i;
            Ok((
                input,
                MBusFrame {
                    frame_type,
                    control,
                    address,
                    control_information,
                    data,
                    checksum,
                    more_records_follow: false,
                },
            ))
        }
    }
}

/// Parses a short M-Bus frame.
fn parse_short_frame(input: &[u8]) -> IResult<&[u8], (u8, FrameData, u8)> {
    // Short frames do not carry control information or data; next byte is checksum.
    let (input, checksum) = be_u8(input)?;
    // Parse and validate stop byte (0x16)
    let (input, stop) = be_u8(input)?;
    if stop != 0x16 {
        return Err(NomErr::Error(nom::error::Error::new(
            input,
            nom::error::ErrorKind::Tag,
        )));
    }
    Ok((input, (0, FrameData::new(), checksum)))
}

/// Parses a control or long M-Bus frame.
fn parse_control_or_long_frame_after_header(
    input: &[u8],
    length1: usize,
) -> IResult<&[u8], (u8, FrameData, u8)> {
    let (input, control_information) = be_u8(input)?;
    let payload_len = length1.saturating_sub(3);
    let (input, data) = take_while_m_n(payload_len, payload_len, |_| true)(input)?;
    let (input, checksum) = be_u8(input)?;
    // Parse and validate stop byte (0x16)
    let (input, stop) = be_u8(input)?;
    if stop != 0x16 {
        return Err(NomErr::Error(nom::error::Error::new(
            input,
            nom::error::ErrorKind::Tag,
        )));
    }
    // A payload longer than MBUS_MAX_DATA cannot be described by the one-byte length
    // field that produced `payload_len`, so this is unreachable for well-formed input —
    // but it is reported rather than truncated, and never panics.
    let data = FrameData::from_slice(data).map_err(|_| {
        NomErr::Error(nom::error::Error::new(
            input,
            nom::error::ErrorKind::TooLarge,
        ))
    })?;
    Ok((input, (control_information, data, checksum)))
}

pub fn pack_frame(frame: &MBusFrame) -> PackedFrame {
    // Every push below is bounded by MBUS_MAX_FRAME, which is derived from the same
    // one-byte length field that bounds `frame.data`. The Results are discarded rather
    // than unwrapped so that packing a frame cannot panic.
    let mut data = PackedFrame::new();

    match frame.frame_type {
        MBusFrameType::Ack => {
            // ACK frame: 0xE5
            let _ = data.push(0xE5);
        }
        MBusFrameType::Short => {
            // Short frame: 0x10 | control | address | checksum | 0x16
            let _ = data.push(0x10);
            let _ = data.push(frame.control);
            let _ = data.push(frame.address);
            // Always compute the checksum from the fields; the supplied `frame.checksum`
            // is advisory (0 for freshly built request frames).
            let _ = data.push(calculate_checksum(frame));
            let _ = data.push(0x16);
        }
        MBusFrameType::Control | MBusFrameType::Long => {
            // Control/Long frame: 0x68 | length1 | length2 | 0x68 | control | address | control_information | data | checksum | 0x16
            pack_control_or_long_frame(&mut data, frame);
        }
    }

    data
}

/// Packs a control or long M-Bus frame into a byte vector.
fn pack_control_or_long_frame(data: &mut PackedFrame, frame: &MBusFrame) {
    let _ = data.push(0x68);
    // Length field is control + address + CI + data, max 255
    // Maximum data length is 252 (255 - 3 for control/address/CI)
    let length = ((frame.data.len() + 3).min(255)) as u8;
    let _ = data.push(length);
    let _ = data.push(length);
    let _ = data.push(0x68);
    let _ = data.push(frame.control);
    let _ = data.push(frame.address);
    let _ = data.push(frame.control_information);
    let _ = data.extend_from_slice(&frame.data);
    // Computed over C + A + CI + data, per EN 13757-2.
    let _ = data.push(calculate_checksum(frame));
    let _ = data.push(0x16);
}

/// Verifies the integrity of an M-Bus frame.
pub fn verify_frame(frame: &MBusFrame) -> Result<(), ProtocolError> {
    let calculated_checksum = calculate_checksum(frame);
    if frame.checksum != calculated_checksum {
        return Err(ProtocolError::InvalidChecksum {
            expected: frame.checksum,
            calculated: calculated_checksum,
        });
    }
    Ok(())
}

/// M-Bus checksum calculation for raw data: the byte sum modulo 256.
///
/// # Arguments
/// * `data` - Raw byte slice to calculate checksum for
///
/// # Returns
/// * Single byte checksum (sum modulo 256)
///
/// A plain wrapping fold. This used to dispatch to a hand-written SIMD routine, which
/// vectorised a `u8` sum across ~250 lines of intrinsics for frames of at most 255 bytes —
/// far below the noise floor of the serial I/O that precedes every one of them, and not
/// portable to a Cortex-M anyway.
pub fn calculate_mbus_checksum(data: &[u8]) -> u8 {
    data.iter().fold(0u8, |acc, &b| acc.wrapping_add(b))
}

/// Calculates the checksum of an M-Bus frame.
fn calculate_checksum(frame: &MBusFrame) -> u8 {
    match frame.frame_type {
        MBusFrameType::Short => calculate_mbus_checksum(&[frame.control, frame.address]),
        MBusFrameType::Control => {
            calculate_mbus_checksum(&[frame.control, frame.address, frame.control_information])
        }
        MBusFrameType::Long => {
            // A wrapping sum needs no buffer: fold the header bytes, then continue over
            // the payload. The previous version allocated a temporary Vec for every
            // checksum, which is also why this module could not build for bare metal.
            let head =
                calculate_mbus_checksum(&[frame.control, frame.address, frame.control_information]);
            frame.data.iter().fold(head, |acc, &b| acc.wrapping_add(b))
        }
        _ => 0,
    }
}

/// Parses the frame type from the input byte slice.
fn parse_frame_type(input: &[u8]) -> IResult<&[u8], (MBusFrameType, Option<u8>)> {
    let (input, start) = be_u8(input)?;
    match start {
        0xE5 => Ok((input, (MBusFrameType::Ack, None))),
        0x10 => Ok((input, (MBusFrameType::Short, None))),
        0x68 => {
            let (input, length1) = be_u8(input)?;
            let (input, length2) = be_u8(input)?;
            if length1 != length2 {
                return Err(NomErr::Error(nom::error::Error::new(
                    input,
                    nom::error::ErrorKind::Tag,
                )));
            }
            let t = if length1 == 3 {
                MBusFrameType::Control
            } else {
                MBusFrameType::Long
            };
            Ok((input, (t, Some(length1))))
        }
        _ => Err(NomErr::Error(nom::error::Error::new(
            input,
            nom::error::ErrorKind::Tag,
        ))),
    }
}

/// Packs a select frame for secondary address selection.
pub fn pack_select_frame(frame: &mut MBusFrame, mask: &str) -> Result<(), ProtocolError> {
    // Pack a 16-hex-digit secondary address mask into 8 bytes following EN 13757-3 specification.
    // Strip whitespace into a fixed 16-byte buffer and upper-case in place. The old
    // version collected into a String and called to_uppercase(); a 16-hex-digit address
    // has a known length, so neither allocation was ever necessary.
    let mut up = [0u8; 16];
    let mut n = 0usize;
    for c in mask.chars().filter(|c| !c.is_whitespace()) {
        if n == 16 || !c.is_ascii_hexdigit() {
            return Err(ProtocolError::InvalidHexString);
        }
        up[n] = (c as u8).to_ascii_uppercase();
        n += 1;
    }
    if n != 16 {
        return Err(ProtocolError::InvalidHexString);
    }
    let hex_digit = |b: u8| -> u8 {
        // Already validated as ASCII hex above.
        if b.is_ascii_digit() {
            b - b'0'
        } else {
            b - b'A' + 10
        }
    };
    let hex_to_byte = |s: &[u8]| (hex_digit(s[0]) << 4) | hex_digit(s[1]);

    let mut data = [0u8; 8];
    // Manufacturer/medium/version
    data[7] = hex_to_byte(&up[14..16]);
    data[6] = hex_to_byte(&up[12..14]);
    let man = ((hex_to_byte(&up[8..10]) as u16) << 8) | hex_to_byte(&up[10..12]) as u16;
    data[4] = ((man >> 8) & 0xFF) as u8;
    data[5] = (man & 0xFF) as u8;
    // ID nibbles with F wildcard support
    data[0] = 0;
    data[1] = 0;
    data[2] = 0;
    data[3] = 0;
    let mut j: i32 = 3;
    let mut k: i32 = 1; // high nibble first
    for &b in up.iter().take(8) {
        let ch = b as char;
        let nibble: u8 = if ch == 'F' {
            0x0F
        } else {
            (ch as u8 - b'0') & 0x0F
        };
        let idx = j as usize;
        data[idx] |= nibble << (4 * k);
        k -= 1;
        if k < 0 {
            k = 1;
            j -= 1;
        }
    }

    // Fill frame fields
    frame.frame_type = MBusFrameType::Long;
    frame.control = MBUS_CONTROL_MASK_SND_UD | MBUS_CONTROL_MASK_FCB;
    frame.address = MBUS_ADDRESS_NETWORK_LAYER;
    frame.control_information = MBUS_CONTROL_INFO_SELECT_SLAVE;
    // `data` is the fixed 8-byte secondary-address block, so this cannot overflow
    // FrameData; reported rather than unwrapped so packing stays panic-free.
    frame.data = FrameData::from_slice(&data)
        .map_err(|_| ProtocolError::InvalidField("select-frame data"))?;

    // Calculate checksum for long frame (control + address + CI + data bytes)
    let mut cksum: u8 = 0;
    cksum = cksum.wrapping_add(frame.control);
    cksum = cksum.wrapping_add(frame.address);
    cksum = cksum.wrapping_add(frame.control_information);
    for b in &frame.data {
        cksum = cksum.wrapping_add(*b);
    }
    frame.checksum = cksum;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn checksum_is_a_wrapping_sum_across_lengths() {
        // Carried over from the SIMD module this replaced: its tests covered the vector
        // lane boundaries (15/16/17, 31/32/33), which are exactly where a vectorised
        // sum would diverge from a scalar one. Equivalence between the two was verified
        // over these cases before the SIMD path was deleted.
        for len in [0usize, 1, 15, 16, 17, 31, 32, 33, 252] {
            let data: heapless::Vec<u8, 252> = (0..len).map(|i| (i * 7 % 256) as u8).collect();
            let expected = data.iter().fold(0u8, |a, &b| a.wrapping_add(b));
            assert_eq!(calculate_mbus_checksum(&data), expected, "len {len}");
        }
    }

    #[test]
    fn checksum_wraps_rather_than_overflowing() {
        assert_eq!(calculate_mbus_checksum(&[0xFF, 0x01]), 0x00);
        assert_eq!(calculate_mbus_checksum(&[0xFF; 4]), 0xFC);
    }

    #[test]
    fn long_frame_checksum_covers_header_and_payload() {
        let frame = MBusFrame {
            frame_type: MBusFrameType::Long,
            control: 0x08,
            address: 0x01,
            control_information: 0x72,
            data: FrameData::from_slice(&[0x11, 0x22, 0x33]).unwrap(),
            checksum: 0,
            more_records_follow: false,
        };
        // C + A + CI + data, per EN 13757-2.
        let expected = 0x08u8
            .wrapping_add(0x01)
            .wrapping_add(0x72)
            .wrapping_add(0x11)
            .wrapping_add(0x22)
            .wrapping_add(0x33);
        assert_eq!(
            pack_frame(&frame)[..].iter().rev().nth(1).copied(),
            Some(expected)
        );
    }
}
