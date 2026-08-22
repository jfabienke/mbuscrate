//! Streaming wM-Bus frame decode — re-exported from `mbus-core`.
//!
//! `FrameDecoder` accumulates bytes from a radio and emits complete frames. It moved to
//! the core once two things were untangled: `packet_size` left the RFM69 driver (it was
//! never radio-specific), and the decoder's `LogThrottle` was removed.
//!
//! That throttle was the decoder's only need for a clock, and it existed purely to
//! rate-limit warnings about errors the decoder was already counting in [`DecodeStats`].
//! Callers that want to report header or CRC errors read `stats()` and throttle their own
//! logging — the same inversion applied to the frame parser's device statistics.
//!
//! The buffer is fixed-capacity now (512 bytes; see `mbus_core::wmbus::decode_buffer`)
//! rather than `util::IoBuffer`, which stays in this crate for its other users.

pub use mbus_core::wmbus::decode_buffer::{
    BufferFull, DecodeBuffer, DecodeBufferStats, DECODE_BUFFER_CAPACITY,
};
pub use mbus_core::wmbus::frame_decode::{
    calculate_wmbus_crc_enhanced, DecodeError, DecodeStats, FrameDecoder, FrameType,
};
