//! Allocation-free protocol errors.
//!
//! Deliberately *not* a port of `mbus_rs::error::MBusError`. That type is a dependency
//! leaf, but not an allocation leaf: eight of its variants carry `String`, constructed at
//! 96 call sites. Moving it here would put the heap back into a crate whose
//! no-allocation property is measured and ratcheted.
//!
//! Almost all of those 96 sites are in transport, device-discovery and manager code that
//! will never leave `mbus-rs` — so the split costs nothing. This type carries only what a
//! *parser* can go wrong with, using `&'static str` where context is needed, exactly as
//! [`crate::lorawan::LoRaWanError`] does. `mbus-rs` converts at the boundary via its own
//! `From<ProtocolError>` impl, so existing callers keep seeing `MBusError`.
//!
//! Variants are added when a ported module needs one, not speculatively.

/// A wire-format error from a pure parsing or packing routine.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum ProtocolError {
    /// Frame checksum did not match the value computed over its fields.
    InvalidChecksum { expected: u8, calculated: u8 },
    /// A hex string was malformed — wrong length, or a non-hex digit.
    InvalidHexString,
    /// A field was outside the range the standard permits.
    InvalidField(&'static str),
}

impl core::fmt::Display for ProtocolError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::InvalidChecksum {
                expected,
                calculated,
            } => write!(
                f,
                "invalid checksum: expected {expected}, calculated {calculated}"
            ),
            Self::InvalidHexString => write!(f, "invalid hexadecimal string"),
            Self::InvalidField(name) => write!(f, "invalid field: {name}"),
        }
    }
}

impl core::error::Error for ProtocolError {}
