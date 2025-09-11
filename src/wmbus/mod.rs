//! # Wireless M-Bus (wM-Bus) Module
//!
//! This module provides the necessary functionality for handling the wireless
//! M-Bus (wM-Bus) protocol, which is an extension of the wired M-Bus protocol
//! for wireless communication with utility meters.
//!
pub mod compact_cache;
pub mod crypto;
pub mod encryption;
pub mod frame;
pub mod frame_decode;
pub mod handle;
pub mod mode_switching;
pub mod network;
pub mod radio;

pub use radio::{
    driver::Sx126xDriver,
    irq::{IrqMaskBit, IrqStatus},
    modulation::{CrcType, GfskModParams, HeaderType, ModulationParams, PacketParams, PacketType},
    radio_driver::{RadioDriver, RadioDriverError, RadioMode, WMBusConfig, ReceivedPacket, RadioStats, DriverInfo},
    rfm69_packet,
    rfm69_registers,
};

// Re-export RFM69 driver when feature is enabled
#[cfg(feature = "rfm69")]
pub use radio::rfm69::{Rfm69Driver, Rfm69Config, Rfm69Error, Rfm69Mode};

// Re-export the necessary types and functions from the submodules
pub use compact_cache::{CompactFrameCache, CachedDeviceInfo, CacheStats};
pub use crypto::{WMBusCrypto, AesKey, EncryptionMode, DeviceInfo, CryptoError};
pub use encryption::WMBusEncryption;
pub use frame::WMBusFrame;
pub use frame_decode::{FrameDecoder, DecodeError, FrameType, DecodeStats, calculate_wmbus_crc_enhanced};
pub use handle::WMBusHandle;
pub use mode_switching::{ModeSwitcher, ModeNegotiator, WMBusMode, SwitchingStats};
pub use network::WMBusNetwork;
