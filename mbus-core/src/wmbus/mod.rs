//! Wireless M-Bus protocol primitives.
//!
//! Everything here is pure: bytes in, bytes out. The radio, the HAL and the frame
//! statistics live in `mbus-rs`; this module knows only the wire format.
//!
//! Ported from `mbus-rs` incrementally. `crc` came first deliberately — it has no
//! dependencies at all, so it proves the move/build/ratchet pipeline end to end before
//! anything load-bearing follows it.

pub mod crc;
