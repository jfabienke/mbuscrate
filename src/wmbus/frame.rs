//! # Wireless M-Bus Frame Handling
//!
//! This module provides parsing and generation of wireless M-Bus frames according
//! to EN 13757-4 standard. It handles frame structure validation, CRC verification,
//! and data extraction from received radio frames.
//!
//! ## Frame Structure
//!
//! Wireless M-Bus frames follow this basic structure:
//! ```text
//! ┌─────────────┬─────────────┬──────────────┬─────────────┬─────────────┐
//! │  L-field    │  C-field    │  M-field     │  A-field    │  V-field    │
//! │  (1 byte)   │  (1 byte)   │  (2 bytes)   │  (4 bytes)  │  (1 byte)   │
//! ├─────────────┼─────────────┼──────────────┼─────────────┼─────────────┤
//! │  T-field    │  CI-field   │  Payload     │  CRC        │             │
//! │  (1 byte)   │  (1 byte)   │  (variable)  │  (2 bytes)  │             │
//! └─────────────┴─────────────┴──────────────┴─────────────┴─────────────┘
//! ```
//!
//! ## CRC Calculation
//!
//! The CRC is CRC-16/EN-13757 (poly 0x3D65, init 0x0000, xorout 0xFFFF); see
//! [`crate::wmbus::crc`] for the canonical implementation and its check value.

use crate::instrumentation::stats::{update_device_error, update_device_success, ErrorType};
use crate::vendors;
use crate::wmbus::crc::read_crc_be;
use thiserror::Error;

#[derive(Debug, Clone)]
pub struct WMBusFrame {
    pub length: u8,
    pub control_field: u8,
    pub manufacturer_id: u16,
    pub device_address: u32,
    pub version: u8,
    pub device_type: u8,
    pub control_info: u8,
    pub payload: Vec<u8>,
    pub crc: u16,
    /// Indicates if frame is encrypted (detected from ACC field)
    pub encrypted: bool,
}

#[derive(Error, Debug, Clone, PartialEq)]
pub enum ParseError {
    #[error("Invalid length field")]
    InvalidLength,
    #[error("Invalid CRC")]
    InvalidCrc,
    #[error("Buffer too short")]
    BufferTooShort,
}

/// Calculate the wM-Bus CRC over `data` (CRC-16/EN-13757).
///
/// Delegates to the canonical implementation in [`crate::wmbus::crc`] (poly 0x3D65,
/// init 0x0000, xorout 0xFFFF, check value 0xC2B7). The previous implementation used
/// a reflected 0x8408 polynomial with a 0x3791 init — a non-standard CRC that no
/// meter transmits; it only ever round-tripped against this crate's own [`add_wmbus_crc`].
///
/// # Examples
///
/// ```rust
/// use mbus_rs::wmbus::frame::calculate_wmbus_crc;
///
/// let frame_data = [0x44, 0x93, 0x15, 0x68, 0x61, 0x05, 0x28, 0x74, 0x37, 0x01, 0x8E];
/// let crc = calculate_wmbus_crc(&frame_data);
/// ```
pub fn calculate_wmbus_crc(data: &[u8]) -> u16 {
    crate::wmbus::crc::calculate_wmbus_crc(data)
}

/// Check if frame is encrypted based on control field and CI
///
/// Encrypted frames are indicated by:
/// - ACC bit (bit 7) set in control field, OR
/// - CI field in range 0x7A-0x8B (encrypted short/long formats)
///
/// # Arguments
///
/// * `control_field` - Frame control field
/// * `control_info` - Control information field (CI)
///
/// # Returns
///
/// * true if frame appears to be encrypted
pub fn is_encrypted_frame(control_field: u8, control_info: u8) -> bool {
    // Check ACC bit (bit 7) in control field
    let acc_bit_set = (control_field & 0x80) != 0;

    // Check for encrypted CI range (0x7A-0x8B covers various encrypted formats)
    let encrypted_ci = matches!(control_info, 0x7A..=0x8B);

    acc_bit_set || encrypted_ci
}

/// Verify CRC of a complete wM-Bus frame.
///
/// Uses the canonical CRC-16/EN-13757 algorithm (via [`calculate_wmbus_crc`]) and reads
/// the trailing CRC **big-endian**, as wM-Bus transmits it (see
/// [`crate::wmbus::crc::read_crc_be`]) — matching [`add_wmbus_crc`] and the block-level
/// path in [`super::mode_c::decode_mode_c`].
///
/// # Note: single vs. block CRC
///
/// This models one trailing CRC over the whole frame — correct for a single-block frame.
/// EN 13757-4 multi-block on-wire frames (first block + each 16-byte data block, each with
/// its own CRC) are decoded by [`super::mode_c::decode_mode_c`] / the streaming
/// [`super::frame_decode::FrameDecoder`], which is the canonical live-RX path.
///
/// # Arguments
///
/// * `frame_data` - Complete frame data including CRC field
pub fn verify_wmbus_crc(frame_data: &[u8]) -> bool {
    if frame_data.len() < 3 {
        return false; // Too short to contain CRC
    }

    // Extract CRC from last 2 bytes (big-endian, as transmitted)
    let frame_crc = read_crc_be(&frame_data[frame_data.len() - 2..]);

    // Calculate CRC over data (excluding the CRC field itself)
    let data_for_crc = &frame_data[..frame_data.len() - 2];
    let calculated_crc = calculate_wmbus_crc(data_for_crc);

    frame_crc == calculated_crc
}

/// Generate CRC for wM-Bus frame data
///
/// Calculates and appends the correct CRC to frame data.
///
/// # Arguments
///
/// * `frame_data` - Frame data without CRC
///
/// # Returns
///
/// * Complete frame data with CRC appended
pub fn add_wmbus_crc(frame_data: &[u8]) -> Vec<u8> {
    let crc = calculate_wmbus_crc(frame_data);
    let mut result = frame_data.to_vec();

    // Append CRC big-endian, as wM-Bus transmits it (see crate::wmbus::crc::read_crc_be)
    result.extend_from_slice(&crc.to_be_bytes());

    result
}

/// Parse a wireless M-Bus frame from raw bytes
///
/// Validates frame structure, extracts all fields, and verifies CRC integrity.
/// Returns a structured representation of the frame if parsing succeeds.
///
/// # Arguments
///
/// * `raw_bytes` - Raw frame data received from radio
///
/// # Returns
///
/// * `Ok(WMBusFrame)` - Successfully parsed frame
/// * `Err(ParseError)` - Parsing failed due to invalid structure or CRC
///
/// # Examples
///
/// ```rust
/// use mbus_rs::wmbus::frame::{parse_wmbus_frame, WMBusFrame};
///
/// // A complete frame, CRC included
/// let raw_frame = WMBusFrame::build(0x44, 0x6815, 0x74280561, 0x37, 0x01, 0x8E, &[0x01, 0x02]);
/// match parse_wmbus_frame(&raw_frame) {
///     Ok(frame) => println!("Parsed frame from device {:#X}", frame.device_address),
///     Err(e) => println!("Parse error: {:?}", e),
/// }
/// ```
/// Centralized classifier for wM-Bus / OMS Control-Information (CI) field values that begin
/// the application layer of a FULL frame (EN 13757-3 §5.5, OMS Vol.2). Kept in one place so
/// the full/compact disambiguation stays exhaustive — earlier a narrow allowlist omitted
/// valid CIs such as `0x73` (fixed-data response) and `0x8E` (ELL) and misrouted those full
/// frames to the compact parser.
pub fn is_application_ci(ci: u8) -> bool {
    matches!(
        ci,
        0x51 | 0x52 | 0x54 | 0x55 | 0x5A | 0x5B | 0x5F | 0x60 | 0x61 // commands / SND-UD / RSP
      | 0x64 | 0x65 | 0x69 | 0x6A | 0x6B | 0x6C | 0x6D | 0x6E | 0x6F // COSEM / date-time
      | 0x70 | 0x71                                                   // application error / alarm
      | 0x72 | 0x73 | 0x74 | 0x75 | 0x76 | 0x77                       // variable/fixed data, long header
      | 0x78 | 0x79 | 0x7A | 0x7B | 0x7C | 0x7D | 0x7E | 0x7F         // no/short header, compact, plain
      | 0x80 | 0x8A | 0x8B | 0x8C | 0x8D | 0x8E | 0x8F                // TPL / ELL
      | 0x90 | 0x91 | 0x92 | 0x93 | 0x94 | 0x95 | 0x96 | 0x97         // AFL
      | 0xA0..=0xB7 // manufacturer-specific
    )
}

/// Heuristic used to disambiguate a full frame from a compact frame (both can carry a `0x79`
/// at byte 2): a full frame is the right length for its L-field and has a valid application
/// CI at the full-frame CI position (byte 10). A frame shorter than 13 bytes cannot be full.
///
/// This cannot be perfect — a compact frame whose payload byte 10 coincidentally looks like a
/// CI is still ambiguous — so callers that *know* the format should use [`parse_wmbus_frame`]
/// (full) or [`parse_compact_frame`] directly rather than relying on auto-detection.
fn looks_like_full_frame(b: &[u8]) -> bool {
    b.len() >= 13 && b.len() == b[0] as usize + 3 && is_application_ci(b[10])
}

pub fn parse_wmbus_frame(raw_bytes: &[u8]) -> Result<WMBusFrame, ParseError> {
    // A compact frame (OMS format B) carries CI=0x79 at byte 2. But in a FULL frame byte 2
    // is the manufacturer-ID low byte, so a plain `byte[2] == 0x79` test misroutes full
    // frames whose manufacturer code ends that way. Only treat it as compact when the bytes
    // do NOT form a well-formed full frame (right length + a plausible CI at byte 10).
    if raw_bytes.len() >= 7 && raw_bytes[2] == 0x79 && !looks_like_full_frame(raw_bytes) {
        return parse_compact_frame(raw_bytes);
    }

    // Minimum frame size: L(1) + C(1) + M(2) + A(4) + V(1) + T(1) + CI(1) + CRC(2) = 13 bytes
    if raw_bytes.len() < 13 {
        return Err(ParseError::BufferTooShort);
    }

    let length = raw_bytes[0];

    // Validate that L-field matches actual frame length
    // L-field represents bytes following the L-field, excluding CRC
    // So: total_length = L-field + 1 (for L-field) + 2 (for CRC)
    let expected_total_len = (length as usize) + 1 + 2; // +1 for L-field, +2 for CRC
    if raw_bytes.len() != expected_total_len {
        return Err(ParseError::InvalidLength);
    }

    // Extract header fields first to check for encryption
    let control_field = raw_bytes[1];
    let manufacturer_id = u16::from_le_bytes([raw_bytes[2], raw_bytes[3]]);
    let device_address =
        u32::from_le_bytes([raw_bytes[4], raw_bytes[5], raw_bytes[6], raw_bytes[7]]);
    let version = raw_bytes[8];
    let device_type = raw_bytes[9];
    let control_info = raw_bytes[10];

    // Check if frame is encrypted
    let encrypted = is_encrypted_frame(control_field, control_info);

    // Always verify the on-wire link-layer CRC first — including for encrypted frames, whose
    // CRC covers the ciphertext and is valid on a correctly received frame. (Previously
    // encrypted frames skipped this and accepted corrupted ciphertext as a valid frame.)
    if !verify_wmbus_crc(raw_bytes) {
        let device_id = format!("{device_address:08X}");
        update_device_error(&device_id, ErrorType::Crc);
        return Err(ParseError::InvalidCrc);
    }

    // Extract payload (everything between CI field and CRC)
    let payload_start = 11;
    let payload_end = raw_bytes.len() - 2; // Exclude 2-byte CRC
    let payload = if payload_end > payload_start {
        raw_bytes[payload_start..payload_end].to_vec()
    } else {
        vec![]
    };

    // Extract CRC from last 2 bytes (big-endian, as transmitted)
    let crc = read_crc_be(&raw_bytes[raw_bytes.len() - 2..]);

    // Track successful frame parsing
    let device_id = format!("{device_address:08X}");
    update_device_success(&device_id);

    Ok(WMBusFrame {
        length,
        control_field,
        manufacturer_id,
        device_address,
        version,
        device_type,
        control_info,
        payload,
        crc,
        encrypted,
    })
}

/// Parse a compact frame (CI=0x79) according to OMS specification
///
/// Compact frames have reduced header for cached device information:
/// - L-field (1 byte)
/// - C-field (1 byte)  
/// - CI=0x79 (1 byte)
/// - Signature (2 bytes) - identifies cached device
/// - Payload (variable)
/// - CRC (2 bytes)
fn parse_compact_frame(raw_bytes: &[u8]) -> Result<WMBusFrame, ParseError> {
    // Minimum compact frame: L(1) + C(1) + CI(1) + Signature(2) + CRC(2) = 7 bytes
    if raw_bytes.len() < 7 {
        return Err(ParseError::BufferTooShort);
    }

    let length = raw_bytes[0];
    let control_field = raw_bytes[1];
    let control_info = raw_bytes[2]; // Should be 0x79

    if control_info != 0x79 {
        return Err(ParseError::InvalidLength);
    }

    // Extract signature (used to lookup cached device info)
    let signature = u16::from_le_bytes([raw_bytes[3], raw_bytes[4]]);

    // Verify CRC
    if !verify_wmbus_crc(raw_bytes) {
        return Err(ParseError::InvalidCrc);
    }

    // Extract payload (everything between signature and CRC)
    let payload_start = 5;
    let payload_end = raw_bytes.len() - 2;
    let payload = if payload_end > payload_start {
        raw_bytes[payload_start..payload_end].to_vec()
    } else {
        vec![]
    };

    // Extract CRC (big-endian, as transmitted)
    let crc = read_crc_be(&raw_bytes[raw_bytes.len() - 2..]);

    // For compact frames, device info would be retrieved from cache using signature
    // Here we use placeholder values - in production, lookup from cache
    Ok(WMBusFrame {
        length,
        control_field,
        manufacturer_id: signature, // Use signature as manufacturer ID placeholder
        device_address: 0,          // Would be retrieved from cache
        version: 0,                 // Would be retrieved from cache
        device_type: 0,             // Would be retrieved from cache
        control_info,
        payload,
        crc,
        encrypted: false, // Compact frames are typically not encrypted
    })
}

/// Parse wM-Bus frame with vendor extension support
///
/// This function adds vendor-specific CI handling for the range 0xA0-0xB7
/// as defined in EN 13757-4 for manufacturer-specific control information.
pub fn parse_wmbus_frame_with_vendor(
    raw_bytes: &[u8],
    manufacturer_id: Option<&str>,
    registry: Option<&vendors::VendorRegistry>,
) -> Result<WMBusFrame, ParseError> {
    let mut frame = parse_wmbus_frame(raw_bytes)?;

    // Check for vendor-specific CI range (0xA0-0xB7)
    if let (Some(mfr_id), Some(reg)) = (manufacturer_id, registry) {
        if frame.control_info >= 0xA0 && frame.control_info <= 0xB7 {
            // Dispatch to vendor hook
            if let Ok(Some(_vendor_record)) =
                vendors::dispatch_ci_hook(reg, mfr_id, frame.control_info, &frame.payload)
            {
                // For now, just mark in payload that vendor handling occurred
                // In a full implementation, we'd convert vendor_record to appropriate format
                let mut modified_payload = vec![0xFF]; // Vendor marker
                modified_payload.extend_from_slice(&frame.payload);
                frame.payload = modified_payload;
            }
        }
    }

    Ok(frame)
}

impl WMBusFrame {
    /// Build a complete wireless M-Bus frame with correct CRC
    ///
    /// Constructs a properly formatted wM-Bus frame from the provided fields
    /// and calculates the correct CRC according to EN 13757-4.
    ///
    /// # Arguments
    ///
    /// * `control_field` - Frame control field
    /// * `manufacturer_id` - Manufacturer identifier (2 bytes, little-endian)
    /// * `device_address` - Device address (4 bytes, little-endian)
    /// * `version` - Device version
    /// * `device_type` - Device type identifier
    /// * `control_info` - Control information field
    /// * `payload` - Frame payload data
    ///
    /// # Returns
    ///
    /// * Raw frame bytes with correct length field and CRC
    ///
    /// # Examples
    ///
    /// ```rust
    /// use mbus_rs::wmbus::frame::WMBusFrame;
    ///
    /// let frame_data = WMBusFrame::build(
    ///     0x44,                    // Control field
    ///     0x6815,                  // Manufacturer ID (Engelmann)
    ///     0x74280561,              // Device address
    ///     0x37,                    // Version
    ///     0x01,                    // Device type
    ///     0x8E,                    // Control info
    ///     &[0x01, 0x02, 0x03],     // Payload
    /// );
    /// ```
    pub fn build(
        control_field: u8,
        manufacturer_id: u16,
        device_address: u32,
        version: u8,
        device_type: u8,
        control_info: u8,
        payload: &[u8],
    ) -> Vec<u8> {
        // Calculate frame length (excluding L-field itself and CRC)
        let frame_length = 1 + 2 + 4 + 1 + 1 + 1 + payload.len(); // C + M + A + V + T + CI + payload
        let l_field = frame_length as u8;

        // Build frame without CRC
        let mut frame = Vec::new();
        frame.push(l_field); // L-field
        frame.push(control_field); // C-field
        frame.extend_from_slice(&manufacturer_id.to_le_bytes()); // M-field (2 bytes)
        frame.extend_from_slice(&device_address.to_le_bytes()); // A-field (4 bytes)
        frame.push(version); // V-field
        frame.push(device_type); // T-field
        frame.push(control_info); // CI-field
        frame.extend_from_slice(payload); // Payload

        // Calculate and append CRC
        add_wmbus_crc(&frame)
    }

    /// Get raw frame bytes with CRC
    ///
    /// Converts this frame structure back to raw bytes that can be transmitted.
    ///
    /// # Returns
    ///
    /// * Complete frame data as bytes
    pub fn to_bytes(&self) -> Vec<u8> {
        // Note: build() creates unencrypted frames by default
        // Encrypted frames should be created through crypto module
        Self::build(
            self.control_field,
            self.manufacturer_id,
            self.device_address,
            self.version,
            self.device_type,
            self.control_info,
            &self.payload,
        )
    }

    /// Verify the CRC of this frame
    ///
    /// Checks if the stored CRC matches the calculated CRC for the frame data.
    ///
    /// # Returns
    ///
    /// * `true` if CRC is valid
    /// * `false` if CRC is invalid
    pub fn verify_crc(&self) -> bool {
        let frame_bytes = self.to_bytes();
        verify_wmbus_crc(&frame_bytes)
    }
}
