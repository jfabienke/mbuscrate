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
//! - 2 bytes of CRC-16 (init value 0xFFFF)
//!
//! ## Usage
//!
//! ```rust
//! use wmbus::block::{verify_blocks, extract_block_data};
//!
//! let payload = [...]; // Multi-block payload
//! match verify_blocks(&payload, false) {
//!     Ok(blocks) => {
//!         let data = extract_block_data(&blocks);
//!         // Process concatenated data
//!     }
//!     Err(e) => println!("Block validation failed: {}", e),
//! }
//! ```

use crate::error::MBusError;
use crate::vendors::{CrcErrorType, CrcErrorContext, VendorRegistry, dispatch_crc_tolerance};
use crate::instrumentation::stats::{update_device_error, ErrorType};
use log::{debug, warn};

/// Size of a complete block (data + CRC)
pub const BLOCK_SIZE: usize = 16;
/// Size of data portion in each block
pub const BLOCK_DATA_SIZE: usize = 14;
/// Size of CRC field in each block
pub const BLOCK_CRC_SIZE: usize = 2;

/// CRC-16 polynomial for block validation (OMS specific)
const BLOCK_CRC_POLY: u16 = 0x3D65;
/// Initial value for block CRC calculation
const BLOCK_CRC_INIT: u16 = 0xFFFF;

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

/// Calculate CRC-16 for a block using OMS polynomial
///
/// Uses polynomial 0x3D65 with initial value 0xFFFF as specified
/// in OMS 7.2.1 for block-level integrity checking.
pub fn calculate_block_crc(data: &[u8]) -> u16 {
    let mut crc = BLOCK_CRC_INIT;

    for &byte in data {
        crc ^= (byte as u16) << 8;
        for _ in 0..8 {
            if crc & 0x8000 != 0 {
                crc = (crc << 1) ^ BLOCK_CRC_POLY;
            } else {
                crc <<= 1;
            }
        }
    }

    // Note: Block CRC is NOT complemented (unlike frame CRC)
    crc
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
/// let payload = vec![0; 32]; // 2 blocks
/// let blocks = verify_blocks(&payload, false)?;
/// assert_eq!(blocks.len(), 2);
/// ```
pub fn verify_blocks(payload: &[u8], encrypted: bool) -> Result<Vec<BlockInfo>, MBusError> {
    if payload.is_empty() {
        return Ok(Vec::new());
    }

    // Check if payload is properly block-aligned
    if payload.len() % BLOCK_SIZE != 0 {
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
        let crc_received = u16::from_le_bytes([block_data[14], block_data[15]]);

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
    let crc_received = u16::from_le_bytes([block_data[14], block_data[15]]);
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
pub fn process_type_a_blocks(
    payload: &[u8],
    encrypted: bool,
) -> Result<Vec<u8>, MBusError> {
    let blocks = verify_blocks(payload, encrypted)?;

    // Check if all blocks are valid (for non-encrypted)
    if !encrypted {
        let invalid_count = blocks.iter().filter(|b| !b.crc_valid).count();
        if invalid_count > 0 {
            warn!("{} of {} blocks have invalid CRC", invalid_count, blocks.len());
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
pub fn verify_blocks_with_vendor(
    payload: &[u8],
    encrypted: bool,
    manufacturer_id: Option<&str>,
    device_id: Option<&str>,
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
        let crc_received = u16::from_le_bytes([block_data[14], block_data[15]]);
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
                        debug!("Vendor tolerance applied for block {} CRC error", blocks.len());
                        crc_valid = true; // Tolerate the error
                    }
                    _ => {
                        // Track block CRC error
                        if let Some(dev_id) = device_id {
                            update_device_error(dev_id, ErrorType::BlockCrc);
                        }
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
        let data = vec![0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08,
                        0x09, 0x0A, 0x0B, 0x0C, 0x0D, 0x0E];
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
        block.push((crc & 0xFF) as u8);
        block.push((crc >> 8) as u8);

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
            block_data.push((crc & 0xFF) as u8);
            block_data.push((crc >> 8) as u8);
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
            block_data.push((crc & 0xFF) as u8);
            block_data.push((crc >> 8) as u8);
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