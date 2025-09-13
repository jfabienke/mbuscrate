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
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PacketType {
    /// Gaussian Frequency Shift Keying - used for wM-Bus and similar protocols
    Gfsk,
    /// Long Range modulation for LoRa
    LoRa,
}

/// Spreading Factor (SF) for LoRa (Table 13-47)
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, serde::Serialize, serde::Deserialize)]
pub enum SpreadingFactor {
    SF5  = 0x05,
    SF6  = 0x06,
    SF7  = 0x07,
    SF8  = 0x08,
    SF9  = 0x09,
    SF10 = 0x0A,
    SF11 = 0x0B,
    SF12 = 0x0C,
}

/// Bandwidth for LoRa (Table 13-48)
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum LoRaBandwidth {
    BW7_8  = 0x00, // 7.8 kHz
    BW10_4 = 0x08, // 10.4 kHz
    BW15_6 = 0x01, // 15.6 kHz
    BW20_8 = 0x09, // 20.8 kHz
    BW31_2 = 0x02, // 31.25 kHz
    BW41_7 = 0x0A, // 41.7 kHz
    BW62_5 = 0x03, // 62.5 kHz
    BW125  = 0x04, // 125 kHz
    BW250  = 0x05, // 250 kHz
    BW500  = 0x06, // 500 kHz
}

/// Coding Rate (CR) for LoRa (Table 13-49)
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum CodingRate {
    CR4_5 = 0x01,
    CR4_6 = 0x02,
    CR4_7 = 0x03,
    CR4_8 = 0x04,
}

/// GFSK modulation parameters
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

/// LoRa modulation parameters
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LoRaModParams {
    pub sf: SpreadingFactor,
    pub bw: LoRaBandwidth,
    pub cr: CodingRate,
    /// Enable Low Data Rate Optimization for SF11/SF12 on 125kHz or lower
    pub low_data_rate_optimize: bool,
}

/// Complete modulation parameter set for the radio
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ModulationParams {
    Gfsk {
        /// GFSK-specific modulation parameters
        params: GfskModParams,
    },
    LoRa {
        /// LoRa-specific modulation parameters
        params: LoRaModParams,
    },
}

/// LoRa packet parameters
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LoRaPacketParams {
    pub preamble_len: u16,      // 8 to 65535 symbols
    pub implicit_header: bool,  // true for implicit, false for explicit
    pub payload_len: u8,        // For implicit header mode
    pub crc_on: bool,
    pub iq_inverted: bool,
}

/// Packet structure configuration parameters
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PacketParams {
    Gfsk {
        /// GFSK-specific packet parameters
        preamble_len: u16,
        header_type: HeaderType,
        payload_len: u8,
        crc_on: bool,
        crc_type: CrcType,
        sync_word_len: u8,
    },
    LoRa {
        /// LoRa-specific packet parameters
        params: LoRaPacketParams,
    },
}

/// LoRa packet status (metadata from received LoRa packets)
#[derive(Debug, Clone, Copy, Default)]
pub struct LoRaPacketStatus {
    pub rssi_pkt_dbm: i16,
    pub snr_pkt_db: f32,
    pub signal_rssi_pkt_dbm: i16,
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
            preamble_bits: 48, // Typical S-mode preamble
            sync_bits: 16,     // 2-byte sync word
            crc_bytes: 2,      // 16-bit CRC
            bitrate: 32768,    // 32.768 kbps chip rate
            encoding: EncodingType::Manchester,
        }
    }

    /// Create a new ToA calculator for T-mode (3-out-of-6)
    pub fn t_mode(frame_bytes: usize) -> Self {
        Self {
            frame_bytes,
            preamble_bits: 48, // Typical T-mode preamble
            sync_bits: 16,     // 2-byte sync word
            crc_bytes: 2,      // 16-bit CRC
            bitrate: 100000,   // 100 kbps chip rate
            encoding: EncodingType::ThreeOutOfSix,
        }
    }

    /// Create a new ToA calculator for C-mode (NRZ)
    pub fn c_mode(frame_bytes: usize) -> Self {
        Self {
            frame_bytes,
            preamble_bits: 48, // Typical C-mode preamble
            sync_bits: 16,     // 2-byte sync word
            crc_bytes: 2,      // 16-bit CRC
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
            EncodingType::Manchester => total_bits * 2, // 2× for Manchester
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
            rssi_threshold: -85,   // -85 dBm threshold
            min_listen_time_ms: 5, // 5ms minimum listen
            max_backoff_ms: 1000,  // 1 second max backoff
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
