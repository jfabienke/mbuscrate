//! VIF parsing — re-exported from `mbus-core`, with the owned-string boundary.
//!
//! `normalize_vib` in the core returns `&'static str`, not `String`. Every unit and
//! quantity originates in a `const` table and `VifInfo` already held them as
//! `&'static str`; the old signature called `.to_string()` on the way out, which is where
//! `MBusRecord`'s owned `unit`/`quantity` came from and, with them, two heap allocations
//! per record.
//!
//! [`normalize_vib_owned`] keeps the previous signature for existing callers, so that
//! allocation now happens once at this boundary instead of inside the parser — and can be
//! dropped later without touching the core.

pub use mbus_core::payload::vif::{normalize_vib, parse_vib, parse_vif, parse_vife, Vib, VifInfo};

use crate::error::MBusError;

/// [`normalize_vib`] with owned strings, for callers that still want `String`.
pub fn normalize_vib_owned(vib: &[VifInfo]) -> Result<(String, f64, String), MBusError> {
    let (unit, scale, quantity) = normalize_vib(vib)?;
    Ok((unit.to_string(), scale, quantity.to_string()))
}
