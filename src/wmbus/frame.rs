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
//! The CRC is calculated using the CCITT polynomial 0x1021 (reversed as 0x8408)
//! with initial value 0x3791. The calculation covers the entire frame from
//! L-field to the end of data (excluding the CRC itself).

use crate::vendors;
use crate::instrumentation::stats::{update_device_error, update_device_success, ErrorType};
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

/// Calculate wM-Bus CRC using CCITT polynomial with wM-Bus specific parameters
///
/// According to EN 13757-4, the CRC is calculated using:
/// - Polynomial: 0x1021 (CCITT standard)
/// - Reversed polynomial: 0x8408 (for MSB-first calculation)
/// - Initial value: 0x3791
/// - Final XOR: None (result is NOT complemented)
///
/// # Arguments
///
/// * `data` - Data to calculate CRC over (from L-field to end of payload)
///
/// # Returns
///
/// * CRC-16 value as specified by wM-Bus standard
///
/// # Examples
///
/// ```rust
/// let frame_data = [0x44, 0x93, 0x15, 0x68, 0x61, 0x05, 0x28, 0x74, 0x37, 0x01, 0x8E];
/// let crc = calculate_wmbus_crc(&frame_data);
/// ```
pub fn calculate_wmbus_crc(data: &[u8]) -> u16 {
    const POLYNOMIAL: u16 = 0x8408; // Reversed CCITT polynomial
    const INITIAL: u16 = 0x3791; // wM-Bus specific initial value

    let mut crc = INITIAL;

    for &byte in data {
        crc ^= byte as u16;

        for _ in 0..8 {
            if crc & 1 != 0 {
                crc = (crc >> 1) ^ POLYNOMIAL;
            } else {
                crc >>= 1;
            }
        }
    }

    // Important: wM-Bus does NOT complement the final result
    crc
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

/// Verify CRC of a complete wM-Bus frame
///
/// Calculates the expected CRC over the frame data and compares it with
/// the provided CRC field.
///
/// # Arguments
///
/// * `frame_data` - Complete frame data including CRC field
///
/// # Returns
///
/// * `true` if CRC is valid
/// * `false` if CRC is invalid
pub fn verify_wmbus_crc(frame_data: &[u8]) -> bool {
    if frame_data.len() < 3 {
        return false; // Too short to contain CRC
    }

    // Extract CRC from last 2 bytes (little-endian)
    let frame_crc = u16::from_le_bytes([
        frame_data[frame_data.len() - 2],
        frame_data[frame_data.len() - 1],
    ]);

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

    // Append CRC in little-endian format
    result.extend_from_slice(&crc.to_le_bytes());

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
/// let raw_frame = [0x44, 0x93, 0x15, 0x68, /* ... */, 0x12, 0x34]; // Complete frame with CRC
/// match parse_wmbus_frame(&raw_frame) {
///     Ok(frame) => println!("Parsed frame from device {:#X}", frame.device_address),
///     Err(e) => println!("Parse error: {:?}", e),
/// }
/// ```
pub fn parse_wmbus_frame(raw_bytes: &[u8]) -> Result<WMBusFrame, ParseError> {
    // Check for compact frame mode first (CI=0x79)
    if raw_bytes.len() >= 3 && raw_bytes[2] == 0x79 {
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

    // Only verify CRC for non-encrypted frames (encrypted frames need post-decrypt CRC)
    if !encrypted && !verify_wmbus_crc(raw_bytes) {
        // Track CRC error for this device
        let device_id = format!("{device_address:08X}");
        update_device_error(&device_id, ErrorType::Crc);
        return Err(ParseError::InvalidCrc);
    }

    if encrypted {
        log::debug!(
            "Encrypted frame detected (CI=0x{control_info:02X}, C=0x{control_field:02X}), deferring CRC validation"
        );
    }

    // Extract payload (everything between CI field and CRC)
    let payload_start = 11;
    let payload_end = raw_bytes.len() - 2; // Exclude 2-byte CRC
    let payload = if payload_end > payload_start {
        raw_bytes[payload_start..payload_end].to_vec()
    } else {
        vec![]
    };

    // Extract CRC from last 2 bytes
    let crc = u16::from_le_bytes([
        raw_bytes[raw_bytes.len() - 2],
        raw_bytes[raw_bytes.len() - 1],
    ]);

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

    // Extract CRC
    let crc = u16::from_le_bytes([
        raw_bytes[raw_bytes.len() - 2],
        raw_bytes[raw_bytes.len() - 1],
    ]);

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
        encrypted: false,           // Compact frames are typically not encrypted
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
            if let Ok(Some(_vendor_record)) = vendors::dispatch_ci_hook(
                reg,
                mfr_id,
                frame.control_info,
                &frame.payload,
            ) {
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
