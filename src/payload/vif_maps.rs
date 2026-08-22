//! VIF lookup tables — re-exported from `mbus-core`.
//!
//! The tables were already `const` and allocation-free. The only thing standing in the
//! way was `10f64.powi`, which lives in `std`; it is replaced by a `const` decimal table
//! in `mbus_core::payload::vif::pow10`, which is also exact where repeated multiplication
//! would accumulate rounding error.

pub use mbus_core::payload::vif_maps::{lookup_primary_vif, lookup_vife_fb, lookup_vife_fd};
