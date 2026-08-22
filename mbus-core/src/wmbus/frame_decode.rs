//! # Streaming wM-Bus frame decoder
//!
//! [`FrameDecoder`] accumulates bytes from a radio, finds each frame boundary, and
//! decodes it. It is a thin streaming shell around two canonical pieces:
//!
//! - [`packet_size`](crate::wmbus::framing::packet_size) determines the total
//!   on-wire length from the header (handling the multi-block CRC count correctly).
//! - [`decode_mode_c`](crate::wmbus::mode_c::decode_mode_c) validates the per-block CRCs
//!   (CRC-16/EN-13757, big-endian), de-blocks the payload, and parses the link header
//!   (BCD address). This is the same code path validated against live meter traffic.
//!
//! ## Input contract
//!
//! Bytes fed to [`add_bytes`](FrameDecoder::add_bytes) must be **normalized** — the
//! sync/type byte first (`0xCD` Type A / `0x3D` Type B), as the RFM69/SX126x drivers
//! deliver them. (Earlier revisions bit-reversed every input byte on the assumption of
//! raw MSB-first delivery; real drivers normalize at the FIFO, so that reversal
//! corrupted already-normalized frames and is gone.)
//!
//! A frame whose block CRCs do not all validate is reported as
//! [`DecodeError::CrcMismatch`] rather than returned; a valid frame is returned as a
//! [`WMBusFrame`] with `control_info` set to the CI byte and `payload` de-blocked.

use crate::wmbus::crc::read_crc_be;
use crate::wmbus::decode_buffer::{DecodeBuffer, DECODE_BUFFER_CAPACITY};
use crate::wmbus::frame::WMBusFrame;
use crate::wmbus::framing::packet_size;
use crate::wmbus::mode_c::decode_mode_c;

/// Enhanced wM-Bus sync word constants for frame type detection
pub mod sync {
    /// Type A sync byte (raw, before bit reversal)
    pub const A_RAW: u8 = 0xB3;
    /// Type B sync byte (raw, before bit reversal)
    pub const B_RAW: u8 = 0xBC;

    /// Type A sync byte (normalized, after bit reversal)
    pub const A_NORM: u8 = 0xCD;
    /// Type B sync byte (normalized, after bit reversal)
    pub const B_NORM: u8 = 0x3D;
}

/// Frame type detected from sync pattern
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FrameType {
    TypeA,
    TypeB,
    Unknown,
}

/// Enhanced frame decoding errors with specific error types
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum DecodeError {
    BufferTooShort {
        needed: usize,
        actual: usize,
    },
    InvalidHeader,
    CrcMismatch {
        expected: u16,
        calculated: u16,
    },
    FrameTooShort {
        frame_type: FrameType,
        length: usize,
    },
    InvalidLength {
        length: u8,
    },
    EncryptionDetected,
    InvalidBlockSize {
        block_num: usize,
        expected: usize,
        actual: usize,
    },
    /// The decode buffer could not accept more bytes.
    ///
    /// Was `ProcessingError { message: String }`, formatted at one call site. The core
    /// has no allocator, and a single formatted string carried no information the variant
    /// name does not — so the one real case became its own variant.
    BufferOverflow,
}

impl core::fmt::Display for DecodeError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::BufferTooShort { needed, actual } => {
                write!(f, "buffer too short: need {needed} bytes, got {actual}")
            }
            Self::InvalidHeader => write!(f, "invalid wM-Bus header: sync patterns not found"),
            Self::CrcMismatch {
                expected,
                calculated,
            } => write!(
                f,
                "CRC validation failed: expected {expected:04X}, calculated {calculated:04X}"
            ),
            Self::FrameTooShort { frame_type, length } => {
                write!(f, "frame too short for type {frame_type:?}: {length} bytes")
            }
            Self::InvalidLength { length } => write!(f, "invalid length field: {length}"),
            Self::EncryptionDetected => write!(
                f,
                "encryption detected: frame requires decryption before CRC validation"
            ),
            Self::InvalidBlockSize {
                block_num,
                expected,
                actual,
            } => write!(
                f,
                "invalid block size: block {block_num} has {actual} bytes, expected {expected}"
            ),
            Self::BufferOverflow => write!(f, "decode buffer full"),
        }
    }
}

impl core::error::Error for DecodeError {}

/// Streaming frame decoder: accumulate bytes, emit decoded frames.
#[derive(Debug)]
pub struct FrameDecoder {
    /// Buffer for accumulating frame data
    buffer: DecodeBuffer,
    /// Frame type of the frame currently being assembled (set once decoded)
    current_frame_type: Option<FrameType>,
    /// Expected frame size once the header has been read
    expected_size: Option<usize>,
    /// Statistics for monitoring
    stats: DecodeStats,
}

/// Statistics for frame decoding operations
#[derive(Debug, Default, Clone, Copy)]
pub struct DecodeStats {
    pub frames_received: u64,
    pub frames_decoded: u64,
    pub crc_errors: u64,
    pub header_errors: u64,
    pub encryption_detected: u64,
    pub type_a_frames: u64,
    pub type_b_frames: u64,
}

impl FrameDecoder {
    /// Create a new frame decoder with default settings
    pub fn new() -> Self {
        Self {
            buffer: DecodeBuffer::new(),
            current_frame_type: None,
            expected_size: None,
            stats: DecodeStats::default(),
        }
    }

    /// Add normalized (sync/type-byte-first) bytes to the decoder buffer.
    ///
    /// See the module-level "Input contract" note: bytes must already be normalized, as
    /// the radio drivers deliver them. No bit reversal is applied.
    pub fn add_bytes(&mut self, data: &[u8]) -> Result<(), DecodeError> {
        self.buffer
            .write(data)
            .map_err(|_| DecodeError::BufferOverflow)?;
        Ok(())
    }

    /// Try to decode a complete frame from the current buffer.
    pub fn try_decode_frame(&mut self) -> Result<Option<WMBusFrame>, DecodeError> {
        loop {
            // Determine frame size if not yet known
            if self.expected_size.is_none() {
                if let Some(size) = self.determine_frame_size()? {
                    self.expected_size = Some(size);
                } else {
                    return Ok(None); // Need more data
                }
            }

            let expected_size = self.expected_size.unwrap();

            // Check if we have enough data
            if self.buffer.len() < expected_size {
                return Ok(None); // Need more data
            }

            // Extract frame data
            let frame_data: heapless::Vec<u8, DECODE_BUFFER_CAPACITY> = self
                .buffer
                .consume_exact(expected_size)
                .map_err(|_| DecodeError::BufferOverflow)?;

            // Reset for next frame
            self.expected_size = None;
            self.stats.frames_received += 1;

            // Delegate validation + structure to the canonical mode-C decoder
            match self.process_frame(&frame_data) {
                Ok(frame) => {
                    self.stats.frames_decoded += 1;
                    return Ok(Some(frame));
                }
                Err(DecodeError::InvalidHeader) => {
                    // Invalid header - clear buffer and try again
                    // No log here, and no throttle to rate-limit one. `stats.header_errors`
                    // already counts this, and a caller that wants to report it reads
                    // `stats()` — which also removes the decoder's only need for a clock,
                    // since throttling requires one.
                    self.stats.header_errors += 1;
                    self.buffer.clear();
                    continue;
                }
                Err(e) => return Err(e),
            }
        }
    }

    /// Determine the total on-wire frame size from the header via the canonical
    /// [`packet_size`], returning `Ok(None)` when more data is needed.
    fn determine_frame_size(&mut self) -> Result<Option<usize>, DecodeError> {
        if self.buffer.len() < 2 {
            return Ok(None); // Need at least 2 bytes for header analysis
        }

        let mut header = [0u8; 2];
        self.buffer.peek(2, &mut header);
        match packet_size(&header) {
            -1 => Ok(None), // Need more data
            n if n < 0 => {
                // Not a wM-Bus header — drop the buffer and signal an invalid header
                self.buffer.clear();
                Err(DecodeError::InvalidHeader)
            }
            n => Ok(Some(n as usize)),
        }
    }

    /// Validate and parse a complete frame via [`decode_mode_c`], converting the result
    /// into a [`WMBusFrame`]. Returns [`DecodeError::CrcMismatch`] if any block CRC fails.
    fn process_frame(&mut self, frame_data: &[u8]) -> Result<WMBusFrame, DecodeError> {
        let link = decode_mode_c(frame_data)?;

        self.current_frame_type = Some(link.frame_type);
        match link.frame_type {
            FrameType::TypeA => self.stats.type_a_frames += 1,
            FrameType::TypeB => self.stats.type_b_frames += 1,
            FrameType::Unknown => {}
        }

        // Trailing CRC as transmitted (big-endian) — used for diagnostics and the
        // WMBusFrame.crc field. For multi-block Type A this is the last block's CRC.
        let trailing_crc = frame_data
            .len()
            .checked_sub(2)
            .map(|i| read_crc_be(&frame_data[i..]))
            .unwrap_or(0);

        if !link.crc_ok {
            // Counted, not logged — see the header-error arm above.
            self.stats.crc_errors += 1;
            return Err(DecodeError::CrcMismatch {
                expected: trailing_crc,
                calculated: 0,
            });
        }

        let control_info = link.ci().unwrap_or(0);
        let encrypted = matches!(control_info, 0x7A | 0x7B | 0x8A | 0x8B);
        if encrypted {
            self.stats.encryption_detected += 1;
        }
        // L-field: for a normalized frame the type byte is at index 0, L at index 1.
        let length = frame_data.get(1).copied().unwrap_or(0);

        Ok(WMBusFrame {
            length,
            control_field: link.control_field,
            manufacturer_id: link.manufacturer_id,
            device_address: link.device_address,
            version: link.version,
            device_type: link.device_type,
            control_info,
            // Fixed-capacity now; an over-long application block is truncated here
            // rather than panicking, and cannot occur for a frame that parsed.
            payload: crate::wmbus::frame::Payload::from_slice(link.application_data())
                .unwrap_or_default(),
            crc: trailing_crc,
            encrypted,
        })
    }

    /// Get current decoding statistics
    pub fn stats(&self) -> DecodeStats {
        self.stats
    }

    /// Reset decoder state
    pub fn reset(&mut self) {
        self.buffer.clear();
        self.current_frame_type = None;
        self.expected_size = None;
    }

    /// Get current buffer statistics
    pub fn buffer_stats(&self) -> crate::wmbus::decode_buffer::DecodeBufferStats {
        self.buffer.stats()
    }
}

impl Default for FrameDecoder {
    fn default() -> Self {
        Self::new()
    }
}

/// Calculate the standard (complemented) wM-Bus CRC-16/EN-13757.
///
/// Delegates to the canonical [`crate::wmbus::crc`] (poly 0x3D65, init 0x0000, xorout
/// 0xFFFF, check value 0xC2B7). Retained as a stable public entry point.
pub fn calculate_wmbus_crc_enhanced(data: &[u8]) -> u16 {
    crate::wmbus::crc::calculate_wmbus_crc(data)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::wmbus::framing::sync_norm;

    /// Local test helpers: `mbus-rs`'s `util::{hex, bitrev}` stay upstairs (they are
    /// String-based and have other consumers), and the core will not take a dependency
    /// on them just to spell a test vector.
    fn hex_to_bytes(s: &str) -> heapless::Vec<u8, 512> {
        let b = s.as_bytes();
        let mut out = heapless::Vec::new();
        let mut i = 0;
        while i + 1 < b.len() {
            let hi = (b[i] as char).to_digit(16).expect("test vector is hex");
            let lo = (b[i + 1] as char).to_digit(16).expect("test vector is hex");
            out.push(((hi << 4) | lo) as u8).expect("test vector fits");
            i += 2;
        }
        out
    }

    /// Reverse the bits of a byte — the transformation a radio applies to the sync word.
    fn rev8(mut b: u8) -> u8 {
        b = (b & 0xF0) >> 4 | (b & 0x0F) << 4;
        b = (b & 0xCC) >> 2 | (b & 0x33) << 2;
        (b & 0xAA) >> 1 | (b & 0x55) << 1
    }

    /// Real, complete Type B frame from meter 74644444 (KAM), CI=0x8D.
    const REAL_TYPE_B: &str =
        "3d25442d2c444464741b168d208d3048a121f6597959d56873b609a439b99d58531a8a726d9f0c";

    #[test]
    fn decodes_real_type_b_end_to_end() {
        let raw = hex_to_bytes(REAL_TYPE_B);
        let mut decoder = FrameDecoder::new();
        decoder.add_bytes(&raw).expect("add_bytes");

        let frame = decoder
            .try_decode_frame()
            .expect("no error")
            .expect("a complete frame");

        assert_eq!(frame.length, 0x25);
        assert_eq!(frame.control_field, 0x44);
        assert_eq!(frame.manufacturer_id, 0x2C2D); // "KAM"
        assert_eq!(frame.device_address, 74_644_444); // BCD-decoded, not binary
        assert_eq!(frame.control_info, 0x8D);

        let stats = decoder.stats();
        assert_eq!(stats.frames_decoded, 1);
        assert_eq!(stats.type_b_frames, 1);
        assert_eq!(stats.crc_errors, 0);
    }

    #[test]
    fn rejects_single_bit_corruption() {
        let mut raw = hex_to_bytes(REAL_TYPE_B);
        raw[15] ^= 0x01; // flip one bit inside block 0 (covered by the CRC)

        let mut decoder = FrameDecoder::new();
        decoder.add_bytes(&raw).expect("add_bytes");

        let result = decoder.try_decode_frame();
        assert!(
            matches!(result, Err(DecodeError::CrcMismatch { .. })),
            "corrupted frame must fail CRC, got {result:?}"
        );
        assert_eq!(decoder.stats().crc_errors, 1);
    }

    #[test]
    fn waits_for_more_data_on_partial_frame() {
        let raw = hex_to_bytes(REAL_TYPE_B);
        let mut decoder = FrameDecoder::new();
        // Feed all but the last 4 bytes: header known, frame incomplete.
        decoder.add_bytes(&raw[..raw.len() - 4]).expect("add_bytes");
        assert!(decoder.try_decode_frame().expect("no error").is_none());

        // Feed the rest; now it decodes.
        decoder.add_bytes(&raw[raw.len() - 4..]).expect("add_bytes");
        assert!(decoder.try_decode_frame().expect("no error").is_some());
    }

    #[test]
    fn rejects_non_wmbus_header() {
        let mut decoder = FrameDecoder::new();
        decoder
            .add_bytes(&[0xFF, 0x12, 0x34, 0x56, 0x78, 0x90])
            .expect("add_bytes");
        assert!(matches!(
            decoder.try_decode_frame(),
            Err(DecodeError::InvalidHeader)
        ));
    }

    #[test]
    fn test_crc_calculation_enhanced() {
        // CRC-16/EN-13757 standard check value: "123456789" -> 0xC2B7.
        assert_eq!(calculate_wmbus_crc_enhanced(b"123456789"), 0xC2B7);
    }

    #[test]
    fn test_frame_decoder_creation() {
        let decoder = FrameDecoder::new();
        assert_eq!(decoder.stats.frames_received, 0);
        assert_eq!(decoder.current_frame_type, None);
        assert_eq!(decoder.expected_size, None);
    }

    #[test]
    fn test_bit_reversal_integration() {
        // Sync byte normalization (raw MSB-first -> normalized) still holds.
        assert_eq!(rev8(sync::A_RAW), sync::A_NORM); // 0xB3 -> 0xCD
        assert_eq!(rev8(sync::B_RAW), sync::B_NORM); // 0xBC -> 0x3D
                                                     // sync_norm maps the raw bytes to normalized directly.
        assert_eq!(sync_norm(sync::A_RAW), sync::A_NORM);
        assert_eq!(sync_norm(sync::B_RAW), sync::B_NORM);
    }
}
