//! Wired M-Bus frame parsing and packing — re-exported from `mbus-core`.
//!
//! The implementation moved to `mbus_core::mbus::frame` so it runs on bare metal. It
//! needed only `constants` and `error`, both of which moved with it. This re-export keeps
//! every existing `mbus_rs::mbus::frame::…` path working.
//!
//! Two behavioural notes from the move:
//!
//! * `MBusFrame::data` is now a fixed-capacity `FrameData` (252 bytes — the maximum the
//!   one-byte length field can describe), not a `Vec<u8>`. It derefs to `[u8]`, so
//!   reading code is unaffected; code that *builds* a frame constructs it differently.
//! * `calculate_mbus_checksum` is a plain wrapping fold. It used to dispatch to a
//!   hand-written SIMD routine; see `mbus_core::mbus::frame` for why that went.

pub use mbus_core::mbus::frame::{
    calculate_mbus_checksum, pack_frame, pack_select_frame, parse_frame, verify_frame, FrameData,
    MBusFrame, MBusFrameType, PackedFrame, MBUS_MAX_DATA, MBUS_MAX_FRAME,
};
