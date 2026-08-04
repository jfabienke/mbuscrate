//! # wM-Bus mode C link-layer decode (Type A / Type B block framing)
//!
//! Given one *complete, normalized* mode-C frame — the bytes a radio (or capture
//! file) delivers, sync/type byte first — this module validates the per-block CRCs,
//! strips them, and reassembles the link header plus the de-blocked application
//! payload. It is the counterpart to [`crate::wmbus::radio::rfm69_packet::packet_size`],
//! which finds the frame boundary; this decodes what's inside it.
//!
//! ## On-wire format
//!
//! The frame begins with a normalized type byte — `0xCD` = Type A, `0x3D` = Type B —
//! followed by the L-field and the link header (C, M, A, V, T), with the application
//! payload split into blocks each protected by its own CRC-16/EN-13757 read
//! **big-endian** ([`crate::wmbus::crc`]):
//!
//! - **Type A**: block 0 = 10 header bytes (L, C, M, M, A, A, A, A, V, T) + CRC; then
//!   16-byte data blocks (last may be short) each + CRC. The de-blocked payload is the
//!   concatenation of the data blocks and begins with the CI byte.
//! - **Type B**: a single CRC covers block 0 = `L-1` bytes from the L-field; the payload
//!   (CI onward) is the tail of block 0 after the 10-byte header. An optional block 2
//!   with its own trailing CRC follows for long telegrams.
//!
//! ## Why this exists separately from [`FrameDecoder`]
//!
//! [`FrameDecoder`](crate::wmbus::frame_decode::FrameDecoder) reads the block CRC
//! little-endian, accepts either the raw or complemented CRC, treats Type A as a single
//! trailing CRC, and reports the address as a raw `u32`. This decoder reflects what real
//! meters (74644444/80504381/63398862) actually transmit, cross-checked against the
//! epulse C++ reference and validated on live 868 MHz traffic. New code should decode
//! mode-C frames through here.

use crate::wmbus::crc::{calculate_wmbus_crc, read_crc_be};
use crate::wmbus::frame_decode::{DecodeError, FrameType};

/// Normalized Type A sync/type byte.
const TYPE_A_SYNC: u8 = 0xCD;
/// Normalized Type B sync/type byte.
const TYPE_B_SYNC: u8 = 0x3D;

/// Link header length in bytes: L, C, M, M, A, A, A, A, V, T.
const HEADER_LEN: usize = 10;
/// Type A data block payload length (before its trailing CRC).
const TYPE_A_BLOCK_LEN: usize = 16;
/// Per-block CRC length.
const CRC_LEN: usize = 2;

/// A decoded wM-Bus mode-C link-layer frame.
///
/// Carries the parsed link header plus the de-blocked application [`payload`](Self::payload)
/// (per-block CRCs stripped), which begins with the CI byte. [`crc_ok`](Self::crc_ok)
/// reports whether *every* block CRC validated.
#[derive(Debug, Clone, PartialEq)]
pub struct WMBusLinkFrame {
    /// Frame framing type — always [`FrameType::TypeA`] or [`FrameType::TypeB`].
    pub frame_type: FrameType,
    /// Link-layer C-field (control).
    pub control_field: u8,
    /// Manufacturer ID (2 bytes, little-endian on the wire). Decode with
    /// [`crate::id_to_manufacturer`].
    pub manufacturer_id: u16,
    /// Meter address, BCD-decoded to its decimal id (e.g. wire `44 44 64 74` → `74644444`).
    pub device_address: u32,
    /// Device version byte.
    pub version: u8,
    /// Device type byte.
    pub device_type: u8,
    /// De-blocked application payload (CRCs stripped); begins with the CI byte when non-empty.
    pub payload: Vec<u8>,
    /// True iff every block CRC validated.
    pub crc_ok: bool,
}

impl WMBusLinkFrame {
    /// The CI (Control Information) byte, i.e. the first payload byte, if any.
    ///
    /// Empty for short frames with no application layer (e.g. ACC-NR / SND-NKE).
    pub fn ci(&self) -> Option<u8> {
        self.payload.first().copied()
    }

    /// Application data following the CI byte (empty if the payload is empty).
    pub fn application_data(&self) -> &[u8] {
        self.payload.get(1..).unwrap_or(&[])
    }
}

/// Validate one block: its trailing CRC (big-endian) against the canonical CRC of `data`.
fn verify_block(data: &[u8], crc_bytes: &[u8]) -> bool {
    crc_bytes.len() >= CRC_LEN && calculate_wmbus_crc(data) == read_crc_be(crc_bytes)
}

/// Decode a 4-byte BCD meter address (little-endian on the wire) to its decimal id.
///
/// Matches the meter id printed on the device and the epulse `DecodeBCD` reference:
/// wire bytes `44 44 64 74` → `74644444`.
fn bcd4(bytes: &[u8]) -> u32 {
    let mut v = 0u32;
    for &b in bytes.iter().rev() {
        v = v * 100 + (b >> 4) as u32 * 10 + (b & 0x0F) as u32;
    }
    v
}

/// Decode a normalized mode-C wM-Bus frame into its link header and de-blocked payload.
///
/// `raw` must start with the normalized type byte (`0xCD` Type A / `0x3D` Type B). The
/// returned [`WMBusLinkFrame::crc_ok`] indicates whether all block CRCs validated; the
/// header fields are returned regardless so callers can log/diff a CRC-failed frame.
///
/// # Errors
///
/// Returns [`DecodeError::InvalidHeader`] for an unrecognized type byte and
/// [`DecodeError::BufferTooShort`] when the frame is truncated below what the header
/// implies.
///
/// # Example
///
/// ```
/// use mbus_rs::wmbus::mode_c::decode_mode_c;
/// // Real Type B frame from meter 74644444 (KAM), CI=0x8D.
/// let raw = hex::decode(
///     "3d25442d2c444464741b168d208d3048a121f6597959d56873b609a439b99d58531a8a726d9f0c",
/// ).unwrap();
/// let f = decode_mode_c(&raw).unwrap();
/// assert_eq!(f.device_address, 74644444);
/// assert!(f.crc_ok);
/// ```
pub fn decode_mode_c(raw: &[u8]) -> Result<WMBusLinkFrame, DecodeError> {
    if raw.len() < 2 {
        return Err(DecodeError::BufferTooShort {
            needed: 2,
            actual: raw.len(),
        });
    }
    let type_a = match raw[0] {
        TYPE_A_SYNC => true,
        TYPE_B_SYNC => false,
        _ => return Err(DecodeError::InvalidHeader),
    };
    if raw.len() < 1 + HEADER_LEN + CRC_LEN {
        return Err(DecodeError::BufferTooShort {
            needed: 1 + HEADER_LEN + CRC_LEN,
            actual: raw.len(),
        });
    }

    // Link header occupies raw[1..11] for both types: L, C, M, M, A, A, A, A, V, T.
    let l = raw[1] as usize;
    let control_field = raw[2];
    let manufacturer_id = u16::from_le_bytes([raw[3], raw[4]]);
    let device_address = bcd4(&raw[5..9]);
    let version = raw[9];
    let device_type = raw[10];

    let mut payload = Vec::new();
    let mut crc_ok;

    if type_a {
        // Block 0: 10 header bytes + CRC.
        crc_ok = verify_block(
            &raw[1..1 + HEADER_LEN],
            &raw[1 + HEADER_LEN..1 + HEADER_LEN + CRC_LEN],
        );
        // Data blocks: `l - 9` application bytes in 16-byte blocks, each + CRC.
        let mut remaining = l as isize - (HEADER_LEN as isize - 1);
        let mut pos = 1 + HEADER_LEN + CRC_LEN;
        while remaining > 0 {
            let blklen = (remaining as usize).min(TYPE_A_BLOCK_LEN);
            if pos + blklen + CRC_LEN > raw.len() {
                crc_ok = false;
                break;
            }
            if !verify_block(
                &raw[pos..pos + blklen],
                &raw[pos + blklen..pos + blklen + CRC_LEN],
            ) {
                crc_ok = false;
            }
            payload.extend_from_slice(&raw[pos..pos + blklen]);
            pos += blklen + CRC_LEN;
            remaining -= blklen as isize;
        }
    } else {
        // Type B: one CRC over block 0 = `l - 1` bytes from the L-field.
        if l < 1 {
            return Err(DecodeError::InvalidLength { length: raw[1] });
        }
        let b0 = l - 1;
        if 1 + b0 + CRC_LEN > raw.len() {
            return Err(DecodeError::BufferTooShort {
                needed: 1 + b0 + CRC_LEN,
                actual: raw.len(),
            });
        }
        crc_ok = verify_block(&raw[1..1 + b0], &raw[1 + b0..1 + b0 + CRC_LEN]);
        // Payload (CI onward) is the tail of block 0, after the 10-byte header.
        if 1 + b0 > 1 + HEADER_LEN {
            payload.extend_from_slice(&raw[1 + HEADER_LEN..1 + b0]);
        }
        // Optional block 2 (with its own trailing CRC) for long telegrams.
        let after = 1 + b0 + CRC_LEN;
        if after + CRC_LEN <= raw.len() {
            let b2 = &raw[after..raw.len() - CRC_LEN];
            if !verify_block(b2, &raw[raw.len() - CRC_LEN..]) {
                crc_ok = false;
            }
            payload.extend_from_slice(b2);
        }
    }

    Ok(WMBusLinkFrame {
        frame_type: if type_a {
            FrameType::TypeA
        } else {
            FrameType::TypeB
        },
        control_field,
        manufacturer_id,
        device_address,
        version,
        device_type,
        payload,
        crc_ok,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decodes_real_type_b_frame() {
        // Real, complete Type B frame captured from meter 74644444 (KAM), CI=0x8D.
        let raw = hex::decode(
            "3d25442d2c444464741b168d208d3048a121f6597959d56873b609a439b99d58531a8a726d9f0c",
        )
        .unwrap();
        let f = decode_mode_c(&raw).unwrap();
        assert_eq!(f.frame_type, FrameType::TypeB);
        assert_eq!(f.device_address, 74_644_444); // BCD-decoded
        assert_eq!(f.manufacturer_id, 0x2C2D); // "KAM" (K=11, A=1, M=13)
        assert_eq!(f.control_field, 0x44);
        assert_eq!(f.ci(), Some(0x8D));
        // Real-frame CRC verification (Phase 1.3): the canonical CRC validates it.
        assert!(f.crc_ok);
    }

    #[test]
    fn rejects_unknown_type_byte() {
        let err = decode_mode_c(&[
            0x00, 0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08, 0x09, 0x0A, 0x0B, 0x0C,
        ])
        .unwrap_err();
        assert_eq!(err, DecodeError::InvalidHeader);
    }

    #[test]
    fn reports_too_short_without_panicking() {
        let err = decode_mode_c(&[0x3D, 0x01]).unwrap_err();
        assert!(matches!(err, DecodeError::BufferTooShort { .. }));
    }

    #[test]
    fn bcd4_matches_reference() {
        // Wire order 44 44 64 74 -> 74644444.
        assert_eq!(bcd4(&[0x44, 0x44, 0x64, 0x74]), 74_644_444);
    }
}
