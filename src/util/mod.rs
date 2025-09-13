//! # Utility Modules
//!
//! This module provides common utility functions and types used throughout
//! the mbus-rs crate, including streaming buffers, bit manipulation, hex
//! encoding/decoding, and enhanced logging patterns.

pub mod bitrev;
pub mod hex;
pub mod iobuffer;
pub mod logging;

// Re-export commonly used types and functions
pub use bitrev::{rev16, rev32, rev8, rev8_slice, rev8_vec, BitContext};
pub use hex::{decode_hex, encode_hex, format_hex_compact, hex_to_bytes, pretty_hex};
pub use iobuffer::{IoBuffer, IoBufferError};
pub use logging::{log_frame_hex, log_frame_structured, LogThrottle, ThrottleManager};
