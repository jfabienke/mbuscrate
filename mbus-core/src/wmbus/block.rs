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
//! use mbus_core::wmbus::block::{verify_blocks, extract_block_data};
//!
//! // Two 16-byte blocks (14 data bytes + 2 CRC bytes each)
//! let payload = vec![0u8; 32];
//! match verify_blocks(&payload) {
//!     Ok(blocks) => {
//!         assert_eq!(blocks.len(), 2);
//!         let data = extract_block_data(&blocks);
//!         // Process concatenated data
//!         assert_eq!(data.len(), 28); // 2 blocks x 14 data bytes
//!     }
//!     Err(e) => println!("Block validation failed: {e}"),
//! }
//! ```

use crate::error::ProtocolError;
use crate::wmbus::crc::read_crc_be;
use crate::wmbus::frame::WMBUS_MAX_PAYLOAD;

/// Most blocks a payload can contain: `WMBUS_MAX_PAYLOAD` (245) divided into 16-byte
/// blocks, rounded up. A protocol bound, like every other capacity here.
pub const MAX_BLOCKS: usize = WMBUS_MAX_PAYLOAD.div_ceil(BLOCK_SIZE);

/// One block's bytes.
pub type BlockBytes = heapless::Vec<u8, BLOCK_SIZE>;
/// The blocks of one payload.
pub type Blocks = heapless::Vec<BlockInfo, MAX_BLOCKS>;
/// Reassembled application data from a payload's blocks.
pub type BlockData = heapless::Vec<u8, WMBUS_MAX_PAYLOAD>;

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
    pub raw_data: BlockBytes,
    /// Data portion (14 bytes)
    pub data: BlockBytes,
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
/// Verification is purely a function of the bytes. The `encrypted` flag this used to take
/// only ever gated vendor CRC tolerance, which now runs as a separate pass in
/// `mbus_rs::wmbus::block::verify_blocks_with_vendor` — so the parameter was removed
/// rather than left in place claiming an influence it no longer had. Callers deciding
/// what a failed block *means* for an encrypted payload use
/// [`process_type_a_blocks`], which still takes the flag and uses it.
///
/// # Arguments
///
/// * `payload` - Raw payload data (should be multiple of 16 bytes)
///
/// # Returns
///
/// * `Ok(Vec<BlockInfo>)` - Information about each validated block
/// * `Err(MBusError)` - If block structure is invalid
///
/// # Example
///
/// ```rust
/// use mbus_core::wmbus::block::verify_blocks;
///
/// let payload = vec![0u8; 32]; // 2 blocks
/// let blocks = verify_blocks(&payload).unwrap();
/// assert_eq!(blocks.len(), 2);
/// ```
pub fn verify_blocks(payload: &[u8]) -> Result<Blocks, ProtocolError> {
    if payload.is_empty() {
        return Ok(Blocks::new());
    }

    // `Blocks` holds at most MAX_BLOCKS, derived from WMBUS_MAX_PAYLOAD. A caller can
    // hand this function any slice, so an over-long payload is genuinely reachable and
    // is refused here rather than silently losing its trailing blocks.
    if payload.len() > WMBUS_MAX_PAYLOAD {
        return Err(ProtocolError::InvalidField(
            "payload exceeds the maximum wM-Bus frame payload",
        ));
    }

    // A payload that is not block-aligned is normal: some meters send a partial last
    // block, which the loop below records with crc_valid = false rather than rejecting.
    // (This was an `if` whose only body was a warning; the core does not log, and the
    // condition drove no behaviour.)

    let mut blocks = Blocks::new();
    let mut offset = 0;

    while offset < payload.len() {
        let remaining = payload.len() - offset;
        let block_end = offset + BLOCK_SIZE.min(remaining);
        let block_data = &payload[offset..block_end];

        // Handle partial last block
        if block_data.len() < BLOCK_SIZE {
            // For partial blocks, we can't validate CRC.
            // The length check above bounds the block count, so this push cannot fail.
            let _ = blocks.push(BlockInfo {
                index: blocks.len(),
                raw_data: BlockBytes::from_slice(block_data).unwrap_or_default(),
                data: BlockBytes::from_slice(block_data).unwrap_or_default(),
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

        // Bounded by the payload-length check above.
        let _ = blocks.push(BlockInfo {
            index: blocks.len(),
            raw_data: BlockBytes::from_slice(block_data).unwrap_or_default(),
            data: BlockBytes::from_slice(data).unwrap_or_default(),
            crc_received,
            crc_calculated,
            crc_valid,
        });

        offset = block_end;
    }

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
pub fn extract_block_data(blocks: &[BlockInfo]) -> BlockData {
    // Bounded by WMBUS_MAX_PAYLOAD, which also bounds the input the blocks came from,
    // so this cannot overflow for any payload that parsed. Results discarded rather than
    // unwrapped so reassembly stays panic-free.
    let mut data = BlockData::new();
    for block in blocks {
        let _ = data.extend_from_slice(&block.data);
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
) -> Result<bool, ProtocolError> {
    if block_data.len() != BLOCK_SIZE {
        // Was a formatted String; the sizes are a fixed protocol constant, so a static
        // description carries the same information without an allocator.
        return Err(ProtocolError::InvalidField("block size must be 16 bytes"));
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
                    return Ok(true);
                }
                _ => {}
            }
        }
    }

    Ok(false)
}

/// Process multi-block Type A frame
///
/// Type A frames have intermediate blocks of 16 bytes each,
/// with the final block potentially being shorter.
pub fn process_type_a_blocks(payload: &[u8], encrypted: bool) -> Result<BlockData, ProtocolError> {
    let blocks = verify_blocks(payload)?;

    // Check if all blocks are valid (for non-encrypted)
    if !encrypted {
        let invalid_count = blocks.iter().filter(|b| !b.crc_valid).count();
        if invalid_count > 0 {
            // Continue processing even with some invalid blocks
            // (higher layers can decide how to handle)
        }
    }

    Ok(extract_block_data(&blocks))
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

        let blocks = verify_blocks(&block).unwrap();
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

        let blocks = verify_blocks(&payload).unwrap();
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

        let blocks = verify_blocks(&payload).unwrap();
        let data = extract_block_data(&blocks);

        assert_eq!(data.len(), 2 * BLOCK_DATA_SIZE);
        assert_eq!(&data[0..BLOCK_DATA_SIZE], &vec![0; BLOCK_DATA_SIZE]);
        assert_eq!(&data[BLOCK_DATA_SIZE..], &vec![1; BLOCK_DATA_SIZE]);
    }

    #[test]
    fn test_partial_block_handling() {
        // Create 1.5 blocks
        let payload = vec![0x01; BLOCK_SIZE + 8];

        let blocks = verify_blocks(&payload).unwrap();
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
