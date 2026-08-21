//! wM-Bus Type-A block verification — re-exported from `mbus-core`, plus the
//! vendor-tolerance variant that cannot live there.
//!
//! Block verification itself is pure and moved to `mbus_core::wmbus::block`. As with
//! `frame.rs`, no vendor hook trait was needed: the tolerance decision is entirely
//! *post-hoc*. Everything `CrcErrorContext` carries — block index, total blocks, expected
//! and received CRC — is derivable from the `BlockInfo`s the core already returns, so the
//! vendor pass runs over the finished result rather than inside the parsing loop.
//!
//! `BlockInfo::{raw_data, data}` are fixed-capacity now (16 bytes, the block size). They
//! deref to `[u8]`, so reading code is unaffected.

pub use mbus_core::wmbus::block::{
    calculate_block_crc, extract_block_data, process_type_a_blocks, validate_block_with_tolerance,
    verify_blocks, BlockBytes, BlockData, BlockInfo, Blocks, BLOCK_CRC_SIZE, BLOCK_DATA_SIZE,
    BLOCK_SIZE, MAX_BLOCKS,
};

use crate::error::MBusError;
use crate::vendors::{dispatch_crc_tolerance, CrcErrorContext, CrcErrorType, VendorRegistry};
use log::debug;

/// Verify blocks, then let a vendor extension forgive individual block-CRC failures.
///
/// Runs [`verify_blocks`] first and applies tolerance to the result. Doing it as a second
/// pass rather than inside the loop is what let the verification itself move to
/// `mbus-core`: the core stays a pure function of the bytes, and the policy — which is
/// manufacturer-specific, registry-driven and inherently `std` — stays here.
///
/// Tolerance is never applied to encrypted payloads: a CRC failure there may be
/// ciphertext corruption, which no vendor quirk excuses.
pub fn verify_blocks_with_vendor(
    payload: &[u8],
    encrypted: bool,
    manufacturer_id: Option<&str>,
    registry: Option<&VendorRegistry>,
) -> Result<Blocks, MBusError> {
    let mut blocks = verify_blocks(payload)?;

    if encrypted {
        return Ok(blocks);
    }
    let (Some(mfr), Some(reg)) = (manufacturer_id, registry) else {
        return Ok(blocks);
    };

    let total = Some(payload.len().div_ceil(BLOCK_SIZE));
    for block in blocks.iter_mut() {
        if block.crc_valid {
            continue;
        }
        let context = CrcErrorContext {
            block_index: Some(block.index),
            total_blocks: total,
            crc_expected: block.crc_calculated,
            crc_received: block.crc_received,
            frame_type: Some("TypeA".to_string()),
            vendor_context: std::collections::HashMap::new(),
        };
        if let Ok(Some(true)) =
            dispatch_crc_tolerance(reg, mfr, None, &CrcErrorType::Block, &context)
        {
            debug!(
                "Vendor tolerance applied for block {} CRC error",
                block.index
            );
            block.crc_valid = true;
        }
    }
    Ok(blocks)
}
