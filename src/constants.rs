//! M-Bus protocol constants — re-exported from `mbus-core`.
//!
//! Moved with `mbus::frame`, which was their only consumer in the parsing path. Pure
//! `const` values with no dependencies, so the move was mechanical.

pub use mbus_core::constants::*;
