//! Wireless M-Bus protocol primitives.
//!
//! Everything here is pure: bytes in, bytes out. The radio, the HAL and the frame
//! statistics live in `mbus-rs`; this module knows only the wire format.
//!
//! Ported from `mbus-rs` incrementally. `crc` came first deliberately — it has no
//! dependencies at all, so it proves the move/build/ratchet pipeline end to end before
//! anything load-bearing follows it.

pub mod block;
pub mod crc;
// Key material and crypto errors. Behind `crypto` because it pulls AES, subtle and
// zeroize — a device that only checks CRCs should not link a cipher to do it.
#[cfg(feature = "crypto")]
pub mod crypto;
pub mod decode_buffer;
#[cfg(feature = "crypto")]
pub mod ell;
pub mod frame;
pub mod frame_decode;
pub mod framing;
#[cfg(feature = "crypto")]
pub mod gcm;
pub mod mode_c;
#[cfg(feature = "crypto")]
pub mod oms;
pub mod tpl;
