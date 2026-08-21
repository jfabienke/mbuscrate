//! Wired M-Bus protocol primitives (EN 13757-2/-3).
//!
//! Frame layout and checksums only. The serial port, the async runtime and the request/
//! response state machine stay in `mbus-rs`.

pub mod frame;
