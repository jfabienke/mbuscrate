//! Wireless M-Bus frame parsing — re-exported from `mbus-core`, plus the vendor-aware
//! variant that cannot live there.
//!
//! `parse_wmbus_frame` and the frame types moved to `mbus_core::wmbus::frame`. The split
//! needed no vendor hook trait: the pure parser never touched `vendors` at all, and the
//! coupling was confined to the one function below, which is a *composition* of the core
//! parser and the vendor registry. Composition belongs on the std side, where the
//! registry lives.
//!
//! `WMBusFrame::payload` is now a fixed-capacity `Payload` (245 bytes — `L(255)` less the
//! 10-byte link header). It derefs to `[u8]`, so reading code is unaffected.

pub use mbus_core::wmbus::frame::{
    add_wmbus_crc, calculate_wmbus_crc, is_application_ci, is_encrypted_frame, parse_wmbus_frame,
    verify_wmbus_crc, FrameBytes, ParseError, Payload, WMBusFrame, WMBUS_MAX_FRAME,
    WMBUS_MAX_PAYLOAD,
};

use crate::vendors;

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
            if let Ok(Some(_vendor_records)) = vendors::dispatch_ci_hook(
                reg,
                mfr_id,
                frame.version,
                frame.device_type,
                frame.control_info,
                &frame.payload,
            ) {
                // For now, just mark in payload that vendor handling occurred
                // In a full implementation, we'd convert vendor_record to appropriate format
                // Payload is fixed-capacity now; a marker byte plus the original may
                // exceed it only if the original was already at the limit, in which case
                // the frame is left untouched rather than silently truncated.
                let mut modified_payload = Payload::new();
                if modified_payload.push(0xFF).is_ok()
                    && modified_payload.extend_from_slice(&frame.payload).is_ok()
                {
                    frame.payload = modified_payload;
                }
            }
        }
    }

    Ok(frame)
}
