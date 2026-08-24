//! # Wireless M-Bus (wM-Bus) Module
//!
//! This module provides the necessary functionality for handling the wireless
//! M-Bus (wM-Bus) protocol, which is an extension of the wired M-Bus protocol
//! for wireless communication with utility meters.
//!
pub mod block;
pub mod compact_cache;
pub mod compact_frame;
pub mod crc;
#[cfg(feature = "crypto")]
pub mod crypto;
#[cfg(feature = "crypto")]
pub mod ell;
pub mod encryption;
pub mod frame;
pub mod frame_decode;
pub mod handle;
pub mod mode_c;
pub mod mode_switching;
pub mod network;
#[cfg(feature = "crypto")]
pub mod oms;
pub mod radio;
pub mod status;
pub mod tpl;
pub mod weak_key_audit;

pub use radio::{
    driver::Sx126xDriver,
    irq::{IrqMaskBit, IrqStatus},
    modulation::{CrcType, GfskModParams, HeaderType, ModulationParams, PacketParams, PacketType},
    radio_driver::{
        DriverInfo, RadioDriver, RadioDriverError, RadioMode, RadioStats, ReceivedPacket,
        WMBusConfig,
    },
    rfm69_packet, rfm69_registers,
};

// Re-export RFM69 driver when feature is enabled
#[cfg(feature = "rfm69")]
pub use radio::rfm69::{Rfm69Config, Rfm69Driver, Rfm69Error, Rfm69Mode};

// Re-export the necessary types and functions from the submodules
pub use compact_cache::{CacheStats, CachedDeviceInfo, CompactFrameCache};
pub use compact_frame::{CompactError, CompactLayoutCache, RecordLayout};
#[cfg(feature = "crypto")]
pub use crypto::{AesKey, CryptoError, DeviceInfo, KeyMode, WMBusCrypto};
#[cfg(feature = "crypto")]
pub use ell::{decrypt_ell_payload, DecryptedEll, EllError, EllHeader, EllSecurity};
pub use encryption::WMBusEncryption;
pub use frame::WMBusFrame;
pub use frame_decode::{
    calculate_wmbus_crc_enhanced, DecodeError, DecodeStats, FrameDecoder, FrameType,
};
pub use handle::WMBusHandle;
pub use mode_c::{decode_mode_c, WMBusLinkFrame};
pub use mode_switching::{ModeNegotiator, ModeSwitcher, SwitchingStats, WMBusMode};
pub use network::WMBusNetwork;
#[cfg(feature = "crypto")]
pub use oms::{decrypt_mode5_cbc, decrypted_ok, mode5_cbc_iv, Mode5Error};
pub use tpl::{parse_tpl_header, TplError, TplHeader};
