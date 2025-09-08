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
//!     data: vec![],
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

use crate::MBusError;
use crate::constants::{
    MBUS_ADDRESS_NETWORK_LAYER, MBUS_CONTROL_INFO_SELECT_SLAVE, MBUS_CONTROL_MASK_FCB,
    MBUS_CONTROL_MASK_SND_UD,
};
use bytes::BytesMut;

/// Represents an M-Bus frame.
#[derive(Debug, PartialEq, Eq)]
pub struct MBusFrame {
    pub frame_type: MBusFrameType,
    pub control: u8,
    pub address: u8,
    pub control_information: u8,
    pub data: Vec<u8>,
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
                data: Vec::new(),
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
fn parse_short_frame(input: &[u8]) -> IResult<&[u8], (u8, Vec<u8>, u8)> {
    // Short frames do not carry control information or data; next byte is checksum.
    let (input, checksum) = be_u8(input)?;
    Ok((input, (0, Vec::new(), checksum)))
}

/// Parses a control or long M-Bus frame.
fn parse_control_or_long_frame_after_header(
    input: &[u8],
    length1: usize,
) -> IResult<&[u8], (u8, Vec<u8>, u8)> {
    let (input, control_information) = be_u8(input)?;
    let payload_len = length1.saturating_sub(3);
    let (input, data) = take_while_m_n(payload_len, payload_len, |_| true)(input)?;
    let (input, checksum) = be_u8(input)?;
    Ok((input, (control_information, data.to_vec(), checksum)))
}

pub fn pack_frame_streaming(frame: &MBusFrame) -> BytesMut {
    let mut buf = BytesMut::with_capacity(256);
    // Streaming pack to avoid large allocations
    match frame.frame_type {
        // ... implement with buf.put_slice instead of Vec
        _ => buf,
    }
}
    let mut data = Vec::new();

    match frame.frame_type {
        MBusFrameType::Ack => {
            // ACK frame: 0xE5
            data.push(0xE5);
        }
        MBusFrameType::Short => {
            // Short frame: 0x10 | control | address | checksum | 0x16
            data.push(0x10);
            data.push(frame.control);
            data.push(frame.address);
            data.push(frame.checksum);
            data.push(0x16);
        }
        MBusFrameType::Control => {
            // Control frame: 0x68 | length1 | length2 | 0x68 | control | address | control_information | data | checksum | 0x16
            pack_control_or_long_frame(&mut data, frame);
        }
        MBusFrameType::Long => {
            // Long frame: 0x68 | length1 | length2 | 0x68 | control | address | control_information | data | checksum | 0x16
            pack_control_or_long_frame(&mut data, frame);
        }
    }

    data
}

/// Packs a control or long M-Bus frame into a byte vector.
fn pack_control_or_long_frame(data: &mut Vec<u8>, frame: &MBusFrame) {
    data.push(0x68);
    data.push(frame.data.len() as u8 + 3);
    data.push(frame.data.len() as u8 + 3);
    data.push(0x68);
    data.push(frame.control);
    data.push(frame.address);
    data.push(frame.control_information);
    data.extend_from_slice(&frame.data);
    data.push(frame.checksum);
    data.push(0x16);
}

/// Verifies the integrity of an M-Bus frame.
pub fn verify_frame(frame: &MBusFrame) -> Result<(), MBusError> {
    let calculated_checksum = calculate_checksum(frame);
    if frame.checksum != calculated_checksum {
        return Err(MBusError::InvalidChecksum {
            expected: frame.checksum,
            calculated: calculated_checksum,
        });
    }
    Ok(())
}

/// Calculates the checksum of an M-Bus frame.
fn calculate_checksum(frame: &MBusFrame) -> u8 {
    let mut checksum: u8 = 0;
    match frame.frame_type {
        MBusFrameType::Short => {
            checksum = checksum.wrapping_add(frame.control);
            checksum = checksum.wrapping_add(frame.address);
        }
        MBusFrameType::Control => {
            checksum = checksum.wrapping_add(frame.control);
            checksum = checksum.wrapping_add(frame.address);
            checksum = checksum.wrapping_add(frame.control_information);
        }
        MBusFrameType::Long => {
            checksum = checksum.wrapping_add(frame.control);
            checksum = checksum.wrapping_add(frame.address);
            checksum = checksum.wrapping_add(frame.control_information);
            for byte in &frame.data {
                checksum = checksum.wrapping_add(*byte);
            }
        }
        _ => {}
    }
    checksum
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
pub fn pack_select_frame(frame: &mut MBusFrame, mask: &str) -> Result<(), MBusError> {
    // Pack a 16-hex-digit secondary address mask into 8 bytes, per libmbus.
    let cleaned: String = mask.chars().filter(|c| !c.is_whitespace()).collect();
    if cleaned.len() != 16 || !cleaned.chars().all(|c| c.is_ascii_hexdigit()) {
        return Err(crate::error::MBusError::InvalidHexString);
    }

    let up = cleaned.to_uppercase();
    let hex_to_byte = |s: &str| u8::from_str_radix(s, 16).unwrap_or(0);

    let mut data = [0u8; 8];
    // Manufacturer/medium/version
    data[7] = hex_to_byte(&up[14..16]);
    data[6] = hex_to_byte(&up[12..14]);
    let man = u16::from_str_radix(&up[8..12], 16).unwrap_or(0);
    data[4] = ((man >> 8) & 0xFF) as u8;
    data[5] = (man & 0xFF) as u8;
    // ID nibbles with F wildcard support
    data[0] = 0;
    data[1] = 0;
    data[2] = 0;
    data[3] = 0;
    let mut j: i32 = 3;
    let mut k: i32 = 1; // high nibble first
    for i in 0..8 {
        let ch = up.as_bytes()[i] as char;
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
    frame.data = data.to_vec();

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
