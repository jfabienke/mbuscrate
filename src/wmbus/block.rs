//! # Multi-block Frame Processing for wM-Bus
//!
//! This module handles block-level CRC validation for Type A and encrypted frames
//! according to OMS specification 7.2.1. Multi-block frames consist of 16-byte blocks
//! (14 data bytes + 2 CRC bytes) for improved error detection in noisy environments.
//!
//! ## Block Structure
//!
//! Each block contains:
//! - 14 bytes of data
//! - 2 bytes of CRC-16/EN-13757, big-endian (see [`crate::wmbus::crc`])
//!
//! Note: this is the uniform 16-byte OMS transport-block model (14 data + 2 CRC). The
//! EN 13757-4 *link-layer* Type A framing that meters transmit — a 10-byte block 0 then
//! 16-byte data blocks — is decoded by [`crate::wmbus::mode_c::decode_mode_c`], which is
//! the canonical on-wire path. Both now read the CRC big-endian.
//!
//! ## Usage
//!
//! ```rust
//! use mbus_rs::wmbus::block::{verify_blocks, extract_block_data};
//!
//! // Two 16-byte blocks (14 data bytes + 2 CRC bytes each)
//! let payload = vec![0u8; 32];
//! match verify_blocks(&payload, false) {
//!     Ok(blocks) => {
//!         assert_eq!(blocks.len(), 2);
//!         let data = extract_block_data(&blocks);
//!         // Process concatenated data
//!         assert_eq!(data.len(), 28); // 2 blocks x 14 data bytes
//!     }
//!     Err(e) => println!("Block validation failed: {e}"),
//! }
//! ```

use crate::error::MBusError;
use crate::vendors::{dispatch_crc_tolerance, CrcErrorContext, CrcErrorType, VendorRegistry};
use crate::wmbus::crc::read_crc_be;
use log::{debug, warn};

/// Size of a complete block (data + CRC)
pub const BLOCK_SIZE: usize = 16;
/// Size of data portion in each block
pub const BLOCK_DATA_SIZE: usize = 14;
/// Size of CRC field in each block
pub const BLOCK_CRC_SIZE: usize = 2;

/// Block validation result
#[derive(Debug, Clone)]
pub struct BlockInfo {
    /// Block index (0-based)
    pub index: usize,
    /// Raw block data (16 bytes)
    pub raw_data: Vec<u8>,
    /// Data portion (14 bytes)
    pub data: Vec<u8>,
    /// CRC from block
    pub crc_received: u16,
    /// Calculated CRC
    pub crc_calculated: u16,
    /// Whether CRC is valid
    pub crc_valid: bool,
}

/// Calculate the CRC-16 for a wM-Bus block.
///
/// Block CRCs use the same CRC-16/EN-13757 as frame CRCs (poly 0x3D65, init 0x0000,
/// xorout 0xFFFF); see [`crate::wmbus::crc`]. The previous implementation here used
/// init 0xFFFF with no final xor, which is a different, non-standard CRC and did not
/// match what meters transmit (nor the epulse reference).
///
/// The stored block CRC is read big-endian via [`crate::wmbus::crc::read_crc_be`],
/// matching the wire order used by [`crate::wmbus::mode_c::decode_mode_c`].
pub fn calculate_block_crc(data: &[u8]) -> u16 {
    crate::wmbus::crc::calculate_wmbus_crc(data)
}

/// Verify multi-block payload integrity
///
/// Processes payload as 16-byte blocks, validating CRC for each block.
/// For encrypted frames, CRC validation may be deferred until after decryption.
///
/// # Arguments
///
/// * `payload` - Raw payload data (should be multiple of 16 bytes)
/// * `encrypted` - Whether payload is encrypted (affects validation strategy)
///
/// # Returns
///
/// * `Ok(Vec<BlockInfo>)` - Information about each validated block
/// * `Err(MBusError)` - If block structure is invalid
///
/// # Example
///
/// ```rust
/// use mbus_rs::wmbus::block::verify_blocks;
///
/// let payload = vec![0u8; 32]; // 2 blocks
/// let blocks = verify_blocks(&payload, false).unwrap();
/// assert_eq!(blocks.len(), 2);
/// ```
pub fn verify_blocks(payload: &[u8], encrypted: bool) -> Result<Vec<BlockInfo>, MBusError> {
    if payload.is_empty() {
        return Ok(Vec::new());
    }

    // Check if payload is properly block-aligned
    if !payload.len().is_multiple_of(BLOCK_SIZE) {
        warn!(
            "Payload length {} is not a multiple of block size {}",
            payload.len(),
            BLOCK_SIZE
        );
        // Some meters have partial last block - handle gracefully
    }

    let mut blocks = Vec::new();
    let mut offset = 0;

    while offset < payload.len() {
        let remaining = payload.len() - offset;
        let block_end = offset + BLOCK_SIZE.min(remaining);
        let block_data = &payload[offset..block_end];

        // Handle partial last block
        if block_data.len() < BLOCK_SIZE {
            debug!(
                "Partial block {} with {} bytes (expected {})",
                blocks.len(),
                block_data.len(),
                BLOCK_SIZE
            );
            // For partial blocks, we can't validate CRC
            blocks.push(BlockInfo {
                index: blocks.len(),
                raw_data: block_data.to_vec(),
                data: block_data.to_vec(),
                crc_received: 0,
                crc_calculated: 0,
                crc_valid: false,
            });
            break;
        }

        // Extract data and CRC portions
        let data = &block_data[0..BLOCK_DATA_SIZE];
        let crc_received = read_crc_be(&block_data[14..16]);

        // Calculate expected CRC
        let crc_calculated = calculate_block_crc(data);
        let crc_valid = crc_received == crc_calculated;

        // Log validation result
        if !crc_valid && !encrypted {
            warn!(
                "Block {} CRC mismatch: expected 0x{:04X}, got 0x{:04X}",
                blocks.len(),
                crc_calculated,
                crc_received
            );
        }

        blocks.push(BlockInfo {
            index: blocks.len(),
            raw_data: block_data.to_vec(),
            data: data.to_vec(),
            crc_received,
            crc_calculated,
            crc_valid,
        });

        offset = block_end;
    }

    debug!(
        "Processed {} blocks from {} bytes payload",
        blocks.len(),
        payload.len()
    );

    Ok(blocks)
}

/// Extract concatenated data from validated blocks
///
/// Combines the data portions of all blocks into a single vector,
/// excluding CRC fields.
///
/// # Arguments
///
/// * `blocks` - Validated block information
///
/// # Returns
///
/// * Concatenated data from all blocks
pub fn extract_block_data(blocks: &[BlockInfo]) -> Vec<u8> {
    let mut data = Vec::with_capacity(blocks.len() * BLOCK_DATA_SIZE);
    for block in blocks {
        data.extend_from_slice(&block.data);
    }
    data
}

/// Validate specific block with tolerance for known issues
///
/// Some manufacturers have known CRC calculation bugs in specific blocks.
/// This function allows validation with tolerance for these known issues.
///
/// # Arguments
///
/// * `block_index` - Index of the block being validated
/// * `block_data` - Raw block data (16 bytes)
/// * `manufacturer_id` - Manufacturer identifier for vendor-specific handling
/// * `allow_tolerance` - Whether to apply vendor-specific tolerance
///
/// # Returns
///
/// * `Ok(true)` - Block is valid or tolerated
/// * `Ok(false)` - Block is invalid and not tolerated
/// * `Err` - Block structure error
pub fn validate_block_with_tolerance(
    block_index: usize,
    block_data: &[u8],
    manufacturer_id: Option<&str>,
    allow_tolerance: bool,
) -> Result<bool, MBusError> {
    if block_data.len() != BLOCK_SIZE {
        return Err(MBusError::Other(format!(
            "Invalid block size: {} (expected {})",
            block_data.len(),
            BLOCK_SIZE
        )));
    }

    let data = &block_data[0..BLOCK_DATA_SIZE];
    let crc_received = read_crc_be(&block_data[14..16]);
    let crc_calculated = calculate_block_crc(data);

    if crc_received == crc_calculated {
        return Ok(true);
    }

    // Apply vendor-specific tolerance if enabled
    if allow_tolerance {
        if let Some(mfr) = manufacturer_id {
            // Known vendor-specific issues
            match mfr {
                "QDS" if block_index == 2 => {
                    // QDS has known issue with block 3 (index 2)
                    debug!("Tolerating known QDS block {} CRC issue", block_index + 1);
                    return Ok(true);
                }
                _ => {}
            }
        }
    }

    warn!(
        "Block {block_index} CRC validation failed: expected 0x{crc_calculated:04X}, got 0x{crc_received:04X}"
    );
    Ok(false)
}

/// Process multi-block Type A frame
///
/// Type A frames have intermediate blocks of 16 bytes each,
/// with the final block potentially being shorter.
pub fn process_type_a_blocks(payload: &[u8], encrypted: bool) -> Result<Vec<u8>, MBusError> {
    let blocks = verify_blocks(payload, encrypted)?;

    // Check if all blocks are valid (for non-encrypted)
    if !encrypted {
        let invalid_count = blocks.iter().filter(|b| !b.crc_valid).count();
        if invalid_count > 0 {
            warn!(
                "{} of {} blocks have invalid CRC",
                invalid_count,
                blocks.len()
            );
            // Continue processing even with some invalid blocks
            // (higher layers can decide how to handle)
        }
    }

    Ok(extract_block_data(&blocks))
}

/// Verify blocks with vendor tolerance support
///
/// Enhanced version that integrates with vendor extension system
/// to tolerate known manufacturer-specific CRC issues.
// The `device_id` parameter was dropped: its only use was keying the per-device stats
// call that lived in the CRC-failure arm. Callers that want to attribute block-CRC
// failures read `crc_valid` off the returned `BlockInfo`s, which carry it already.
pub fn verify_blocks_with_vendor(
    payload: &[u8],
    encrypted: bool,
    manufacturer_id: Option<&str>,
    registry: Option<&VendorRegistry>,
) -> Result<Vec<BlockInfo>, MBusError> {
    if payload.is_empty() {
        return Ok(Vec::new());
    }

    let mut blocks = Vec::new();
    let mut offset = 0;

    while offset < payload.len() {
        let remaining = payload.len() - offset;
        let block_end = offset + BLOCK_SIZE.min(remaining);
        let block_data = &payload[offset..block_end];

        // Handle partial last block
        if block_data.len() < BLOCK_SIZE {
            debug!(
                "Partial block {} with {} bytes",
                blocks.len(),
                block_data.len()
            );
            blocks.push(BlockInfo {
                index: blocks.len(),
                raw_data: block_data.to_vec(),
                data: block_data.to_vec(),
                crc_received: 0,
                crc_calculated: 0,
                crc_valid: false,
            });
            break;
        }

        // Extract data and CRC portions
        let data = &block_data[0..BLOCK_DATA_SIZE];
        let crc_received = read_crc_be(&block_data[14..16]);
        let crc_calculated = calculate_block_crc(data);
        let mut crc_valid = crc_received == crc_calculated;

        // Check vendor tolerance if CRC failed
        if !crc_valid && !encrypted {
            if let (Some(mfr), Some(reg)) = (manufacturer_id, registry) {
                let context = CrcErrorContext {
                    block_index: Some(blocks.len()),
                    total_blocks: Some(payload.len().div_ceil(BLOCK_SIZE)),
                    crc_expected: crc_calculated,
                    crc_received,
                    frame_type: Some("TypeA".to_string()),
                    vendor_context: std::collections::HashMap::new(),
                };

                match dispatch_crc_tolerance(reg, mfr, None, &CrcErrorType::Block, &context) {
                    Ok(Some(true)) => {
                        debug!(
                            "Vendor tolerance applied for block {} CRC error",
                            blocks.len()
                        );
                        crc_valid = true; // Tolerate the error
                    }
                    _ => {
                        // No stats call here. Every returned `BlockInfo` already carries
                        // `crc_valid`, so a caller that wants to count block-CRC failures
                        // reads them off the result — the parser recording them itself was
                        // duplicating information it was about to return anyway, and made
                        // a pure function mutate process-wide state.
                        warn!(
                            "Block {} CRC mismatch: expected 0x{:04X}, got 0x{:04X}",
                            blocks.len(),
                            crc_calculated,
                            crc_received
                        );
                    }
                }
            }
        }

        blocks.push(BlockInfo {
            index: blocks.len(),
            raw_data: block_data.to_vec(),
            data: data.to_vec(),
            crc_received,
            crc_calculated,
            crc_valid,
        });

        offset = block_end;
    }

    debug!(
        "Processed {} blocks from {} bytes payload",
        blocks.len(),
        payload.len()
    );

    Ok(blocks)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_block_crc_calculation() {
        // Test vector with known CRC
        let data = vec![
            0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08, 0x09, 0x0A, 0x0B, 0x0C, 0x0D, 0x0E,
        ];
        let crc = calculate_block_crc(&data);

        // Verify CRC is calculated (exact value depends on polynomial)
        assert!(crc != 0x0000);
        assert!(crc != 0xFFFF);
    }

    #[test]
    fn test_verify_single_block() {
        // Create a valid block
        let mut block = vec![0x01; BLOCK_DATA_SIZE];
        let crc = calculate_block_crc(&block);
        block.extend_from_slice(&crc.to_be_bytes()); // big-endian, as transmitted

        let blocks = verify_blocks(&block, false).unwrap();
        assert_eq!(blocks.len(), 1);
        assert!(blocks[0].crc_valid);
    }

    #[test]
    fn test_verify_multiple_blocks() {
        // Create 3 valid blocks
        let mut payload = Vec::new();
        for i in 0..3 {
            let mut block_data = vec![i as u8; BLOCK_DATA_SIZE];
            let crc = calculate_block_crc(&block_data);
            block_data.extend_from_slice(&crc.to_be_bytes()); // big-endian, as transmitted
            payload.extend_from_slice(&block_data);
        }

        let blocks = verify_blocks(&payload, false).unwrap();
        assert_eq!(blocks.len(), 3);
        assert!(blocks.iter().all(|b| b.crc_valid));
    }

    #[test]
    fn test_extract_block_data() {
        // Create blocks with sequential data
        let mut payload = Vec::new();
        for i in 0..2 {
            let mut block_data = vec![i; BLOCK_DATA_SIZE];
            let crc = calculate_block_crc(&block_data);
            block_data.extend_from_slice(&crc.to_be_bytes()); // big-endian, as transmitted
            payload.extend_from_slice(&block_data);
        }

        let blocks = verify_blocks(&payload, false).unwrap();
        let data = extract_block_data(&blocks);

        assert_eq!(data.len(), 2 * BLOCK_DATA_SIZE);
        assert_eq!(&data[0..BLOCK_DATA_SIZE], &vec![0; BLOCK_DATA_SIZE]);
        assert_eq!(&data[BLOCK_DATA_SIZE..], &vec![1; BLOCK_DATA_SIZE]);
    }

    #[test]
    fn test_partial_block_handling() {
        // Create 1.5 blocks
        let payload = vec![0x01; BLOCK_SIZE + 8];

        let blocks = verify_blocks(&payload, false).unwrap();
        assert_eq!(blocks.len(), 2);
        assert_eq!(blocks[1].raw_data.len(), 8); // Partial block
    }

    #[test]
    fn test_vendor_tolerance() {
        // Create invalid block
        let mut block = vec![0x01; BLOCK_DATA_SIZE];
        block.push(0xFF); // Wrong CRC
        block.push(0xFF);

        // Without tolerance
        let valid = validate_block_with_tolerance(0, &block, None, false).unwrap();
        assert!(!valid);

        // With QDS tolerance for block 3 (index 2)
        let valid = validate_block_with_tolerance(2, &block, Some("QDS"), true).unwrap();
        assert!(valid); // Should be tolerated
    }
}
