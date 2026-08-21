//! # M-Bus Error Handling
//!
//! This module defines the MBusError enum, which represents the different error
//! types that can occur in the mbus-rs crate.
//!
//! # Why this has not moved to `mbus-core`
//!
//! It is a dependency leaf — nothing here needs any other module — but it is **not an
//! allocation leaf**: eight variants carry `String`, built at 96 call sites across the
//! crate. Porting it as-is would put the heap back into a core whose no-allocation
//! property is measured and ratcheted in CI.
//!
//! The plan is to split rather than move: `mbus-core` gets a small allocation-free
//! protocol error (`&'static str` context, in the shape of `lorawan::LoRaWanError`),
//! and this type gains a `From` impl for it at the boundary. Almost all 96 sites are in
//! transport, discovery and manager code that will never leave `mbus-rs`, so they are
//! untouched by that split.

use thiserror::Error;

#[derive(Debug, Error)]
pub enum MBusError {
    /// Indicates an error related to the serial port communication.
    #[error("Serial port error: {0}")]
    SerialPortError(String),

    /// Indicates an error when parsing an M-Bus frame.
    #[error("Error parsing M-Bus frame: {0}")]
    FrameParseError(String),

    /// Indicates an unknown Value Information Field (VIF) was encountered.
    #[error("Unknown VIF: 0x{0:02X}")]
    UnknownVif(u8),

    /// Indicates an unknown Value Information Extension Field (VIFE) was encountered.
    #[error("Unknown VIFE: 0x{0:02X}")]
    UnknownVife(u8),

    /// Indicates an invalid hexadecimal string was provided.
    #[error("Invalid hexadecimal string")]
    InvalidHexString,

    /// Indicates an invalid manufacturer ID.
    #[error("Invalid manufacturer")]
    InvalidManufacturer,

    /// Indicates an unknown DIF.
    #[error("Unknown DIF: 0x{0:02X}")]
    UnknownDif(u8),

    /// Indicates VIF is too long.
    #[error("VIF too long")]
    VifTooLong,

    /// Indicates a nom parsing error.
    #[error("Nom error: {0}")]
    NomError(String),

    /// Indicates a device discovery error.
    #[error("Device discovery error: {0}")]
    DeviceDiscoveryError(String),

    /// Indicates a checksum mismatch.
    #[error("Invalid checksum: expected {expected}, calculated {calculated}")]
    InvalidChecksum { expected: u8, calculated: u8 },

    /// Indicates a premature end of data.
    #[error("Premature end of data")]
    PrematureEndAtData,

    /// A catch‑all error for uncategorized cases.
    #[error("Other error: {0}")]
    Other(String),

    /// Invalid manufacturer ID value.
    #[error("Invalid manufacturer id")]
    InvalidManufacturerId,

    /// Wireless M-Bus (wM-Bus) related error
    #[error("Wireless M-Bus error: {0}")]
    WMBusError(String),
}
