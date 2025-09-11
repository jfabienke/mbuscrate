//! # SX126x Modulation and Packet Configuration
//!
//! This module defines the modulation schemes and packet parameters supported by the SX126x radio.
//! Currently focused on GFSK (Gaussian Frequency Shift Keying) modulation which is used for
//! wireless M-Bus and other sub-GHz applications.
//!
//! ## Modulation Types
//!
//! The SX126x supports multiple modulation schemes:
//! - **GFSK**: Gaussian Frequency Shift Keying (implemented)
//! - **LoRa**: Long Range modulation (future implementation)
//!
//! ## GFSK Parameters
//!
//! GFSK modulation requires careful tuning of several parameters:
//!
//! - **Bitrate**: Data transmission rate in bits per second
//! - **Frequency Deviation**: How far the carrier shifts from center frequency
//! - **Bandwidth**: Receiver filter bandwidth
//! - **Modulation Shaping**: Gaussian filter to reduce spectral sidebands
//!
//! ## Packet Structure
//!
//! SX126x packets have this general structure:
//! ```text
//! ┌──────────-┐ ┌────────────┐ ┌────────┐ ┌────────────┐ ┌───────┐
//! │ Preamble  │ │ Sync Word  │ │ Header │ │  Payload   │ │ CRC   │
//! │ (var len) │ │ (1-8 bytes)│ │ (opt.) │ │ (0-255 B)  │ │(1-2B) │
//! └─────────-─┘ └────────────┘ └────────┘ └────────────┘ └───────┘
//! ```
//!
//! ## wM-Bus Configuration Example
//!
//! ```rust,no_run
//! use crate::wmbus::radio::modulation::*;
//!
//! // Typical wM-Bus S-mode configuration
//! let mod_params = ModulationParams {
//!     packet_type: PacketType::Gfsk,
//!     params: GfskModParams {
//!         bitrate: 100_000,        // 100 kbps
//!         modulation_shaping: 1,   // Gaussian 0.5
//!         bandwidth: 156,          // 156 kHz RX bandwidth
//!         fdev: 50_000,           // 50 kHz frequency deviation
//!     },
//! };
//!
//! let packet_params = PacketParams {
//!     packet_type: PacketType::Gfsk,
//!     preamble_len: 48,                    // 48-bit preamble
//!     header_type: HeaderType::Variable,   // Variable length packets
//!     payload_len: 255,                    // Max payload size
//!     crc_on: true,                        // Enable CRC
//!     crc_type: CrcType::Byte2,           // 2-byte CRC
//!     sync_word_len: 4,                    // 4-byte sync word
//! };
//! ```

/// Radio packet type selection
///
/// Defines the fundamental modulation scheme used by the radio.
/// Currently only GFSK is implemented, with LoRa planned for future versions.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PacketType {
    /// Gaussian Frequency Shift Keying - used for wM-Bus and similar protocols
    Gfsk,
    // LoRa modulation support planned for future implementation
}

/// GFSK modulation parameters
///
/// This structure defines all the parameters needed to configure GFSK modulation
/// on the SX126x radio. Proper parameter selection is critical for reliable
/// communication and regulatory compliance.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GfskModParams {
    /// Data transmission rate in bits per second
    ///
    /// Range: 600 bps to 300 kbps (depending on crystal frequency)
    /// Common values: 1200, 4800, 9600, 38400, 100000
    pub bitrate: u32,

    /// Gaussian filter configuration for spectral shaping
    ///
    /// Values:
    /// - 0: No shaping (not recommended)
    /// - 1: Gaussian BT=0.5 (balanced performance)
    /// - 2: Gaussian BT=1.0 (better spectral efficiency)
    /// - 3: Gaussian BT=0.3 (better noise performance)
    pub modulation_shaping: u8,

    /// Receiver bandwidth in kHz
    ///
    /// Should satisfy: BW ≥ 2 × (fdev + bitrate/2)
    pub bandwidth: u8,

    /// Frequency deviation in Hz
    ///
    /// Range: Typically 600 Hz to 200 kHz
    pub fdev: u32,
}

/// Complete modulation parameter set for the radio
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ModulationParams {
    /// The modulation scheme to use
    pub packet_type: PacketType,
    /// GFSK-specific modulation parameters
    pub params: GfskModParams,
}

/// Packet structure configuration parameters
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PacketParams {
    /// The packet type (must match modulation type)
    pub packet_type: PacketType,

    /// Preamble length in bits
    ///
    /// Range: 8 to 65535 bits
    /// Typical values: 16-64 bits
    pub preamble_len: u16,

    /// Packet header configuration
    pub header_type: HeaderType,

    /// Maximum payload length in bytes (0 to 255)
    pub payload_len: u8,

    /// Enable CRC error detection
    pub crc_on: bool,

    /// CRC length configuration
    pub crc_type: CrcType,

    /// Sync word length in bytes (1 to 8)
    pub sync_word_len: u8,
}

/// Packet header type configuration
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HeaderType {
    /// Variable length packets - length field included in header
    Variable,

    /// Fixed length packets - no length field needed
    Fixed,
}

/// CRC (Cyclic Redundancy Check) length configuration
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CrcType {
    /// 1-byte CRC (8-bit) - basic error detection
    Byte1,

    /// 2-byte CRC (16-bit) - robust error detection
    Byte2,
}

/// Time-on-Air (ToA) Calculator for wM-Bus modes
///
/// Calculates transmission time to ensure duty cycle compliance per EN 13757-4.
/// Different wM-Bus modes use different encoding schemes that affect ToA:
/// - S-mode: Manchester encoding (2× overhead)
/// - T-mode: 3-out-of-6 encoding (1.6× overhead)
#[derive(Debug, Clone, Copy)]
pub struct TimeOnAir {
    /// Frame length in bytes
    pub frame_bytes: usize,
    /// Preamble length in bits
    pub preamble_bits: usize,
    /// Sync word length in bits
    pub sync_bits: usize,
    /// CRC length in bytes
    pub crc_bytes: usize,
    /// Bitrate in bits per second
    pub bitrate: u32,
    /// Encoding type
    pub encoding: EncodingType,
}

/// Encoding types for different wM-Bus modes
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum EncodingType {
    /// Manchester encoding (S-mode): 2 chips per bit
    Manchester,
    /// 3-out-of-6 encoding (T-mode): 6 chips per 4 bits = 1.5× overhead
    ThreeOutOfSix,
    /// NRZ encoding (C-mode): No encoding overhead
    Nrz,
    /// No encoding (raw)
    None,
}

impl TimeOnAir {
    /// Create a new ToA calculator for S-mode (Manchester)
    pub fn s_mode(frame_bytes: usize) -> Self {
        Self {
            frame_bytes,
            preamble_bits: 48,  // Typical S-mode preamble
            sync_bits: 16,      // 2-byte sync word
            crc_bytes: 2,       // 16-bit CRC
            bitrate: 32768,    // 32.768 kbps chip rate
            encoding: EncodingType::Manchester,
        }
    }
    
    /// Create a new ToA calculator for T-mode (3-out-of-6)
    pub fn t_mode(frame_bytes: usize) -> Self {
        Self {
            frame_bytes,
            preamble_bits: 48,  // Typical T-mode preamble
            sync_bits: 16,      // 2-byte sync word
            crc_bytes: 2,       // 16-bit CRC
            bitrate: 100000,   // 100 kbps chip rate
            encoding: EncodingType::ThreeOutOfSix,
        }
    }
    
    /// Create a new ToA calculator for C-mode (NRZ)
    pub fn c_mode(frame_bytes: usize) -> Self {
        Self {
            frame_bytes,
            preamble_bits: 48,  // Typical C-mode preamble
            sync_bits: 16,      // 2-byte sync word
            crc_bytes: 2,       // 16-bit CRC
            bitrate: 100000,   // 100 kbps data rate (no chip encoding)
            encoding: EncodingType::Nrz,
        }
    }
    
    /// Calculate time-on-air in milliseconds
    pub fn calculate_ms(&self) -> f64 {
        // Total bits before encoding
        let data_bits = (self.frame_bytes + self.crc_bytes) * 8;
        let total_bits = self.preamble_bits + self.sync_bits + data_bits;
        
        // Apply encoding overhead
        let encoded_bits = match self.encoding {
            EncodingType::Manchester => total_bits * 2,        // 2× for Manchester
            EncodingType::ThreeOutOfSix => (total_bits * 3) / 2, // 1.5× for 3-out-of-6
            EncodingType::Nrz | EncodingType::None => total_bits, // No overhead for NRZ/None
        };
        
        // Calculate time in seconds, then convert to milliseconds
        (encoded_bits as f64 / self.bitrate as f64) * 1000.0
    }
    
    /// Check if transmission meets duty cycle requirement (<0.9% in 1 hour)
    pub fn check_duty_cycle(&self, transmissions_per_hour: u32) -> bool {
        let toa_ms = self.calculate_ms();
        let total_ms_per_hour = toa_ms * transmissions_per_hour as f64;
        let duty_cycle = total_ms_per_hour / (3600.0 * 1000.0); // Hour in ms
        
        duty_cycle < 0.009 // Less than 0.9%
    }
    
    /// Calculate maximum transmissions per hour for duty cycle compliance
    pub fn max_transmissions_per_hour(&self) -> u32 {
        let toa_ms = self.calculate_ms();
        let max_ms_per_hour = 3600.0 * 1000.0 * 0.009; // 0.9% of an hour
        (max_ms_per_hour / toa_ms) as u32
    }
}

/// Listen Before Talk (LBT) support for duty cycle compliance
#[derive(Debug, Clone)]
pub struct ListenBeforeTalk {
    /// RSSI threshold in dBm (typically -85 dBm per ETSI)
    pub rssi_threshold: i16,
    /// Minimum listening time in milliseconds (typically 5ms)
    pub min_listen_time_ms: u32,
    /// Maximum backoff time in milliseconds
    pub max_backoff_ms: u32,
    /// Current backoff counter
    pub backoff_counter: u32,
}

impl ListenBeforeTalk {
    /// Create new LBT with standard ETSI parameters
    pub fn new_etsi() -> Self {
        Self {
            rssi_threshold: -85,      // -85 dBm threshold
            min_listen_time_ms: 5,     // 5ms minimum listen
            max_backoff_ms: 1000,      // 1 second max backoff
            backoff_counter: 0,
        }
    }
    
    /// Check if channel is clear based on RSSI
    pub fn is_channel_clear(&self, rssi_dbm: i16) -> bool {
        rssi_dbm < self.rssi_threshold
    }
    
    /// Calculate backoff time with exponential backoff
    pub fn calculate_backoff_ms(&mut self) -> u32 {
        let backoff = (2_u32.pow(self.backoff_counter) * 10).min(self.max_backoff_ms);
        self.backoff_counter = (self.backoff_counter + 1).min(10); // Cap at 2^10
        backoff
    }
    
    /// Reset backoff counter on successful transmission
    pub fn reset_backoff(&mut self) {
        self.backoff_counter = 0;
    }
    
    /// Calculate total channel access time including LBT
    pub fn total_access_time_ms(&self, toa_ms: f64) -> f64 {
        self.min_listen_time_ms as f64 + toa_ms
    }
}

#[cfg(test)]
#[path = "modulation_tests.rs"]
mod tests;
