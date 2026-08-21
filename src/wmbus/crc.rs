//! Canonical wireless M-Bus CRC — re-exported from `mbus-core`.
//!
//! The implementation moved to `mbus_core::wmbus::crc` so it can run on bare metal; it
//! had no dependencies at all, which made it the right first module to move. This
//! re-export keeps every existing `mbus_rs::wmbus::crc::…` path working, so the move is
//! an internal reorganisation rather than an API break.

pub use mbus_core::wmbus::crc::{calculate_wmbus_crc, read_crc_be, verify_crc_be, WMBUS_CRC_POLY};
