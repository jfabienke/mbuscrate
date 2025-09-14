//! LoRa-specific parameters and helper functions for SX126x configuration.

// Re-export types from modulation module to avoid duplication
pub use crate::wmbus::radio::modulation::{
    SpreadingFactor, LoRaBandwidth, CodingRate, LoRaModParams, LoRaPacketParams
};

/// Calculate LoRa bitrate in bps (datasheet formula)
pub fn lora_bitrate_hz(sf: SpreadingFactor, bw: LoRaBandwidth, cr: CodingRate) -> f64 {
    use crate::wmbus::radio::modulation::{SpreadingFactor, LoRaBandwidth, CodingRate};
    let sf_num = match sf {
        SpreadingFactor::SF5 => 5,
        SpreadingFactor::SF6 => 6,
        SpreadingFactor::SF7 => 7,
        SpreadingFactor::SF8 => 8,
        SpreadingFactor::SF9 => 9,
        SpreadingFactor::SF10 => 10,
        SpreadingFactor::SF11 => 11,
        SpreadingFactor::SF12 => 12,
    };
    let bw_hz = match bw {
        LoRaBandwidth::BW7_8 => 7800.0,
        LoRaBandwidth::BW10_4 => 10400.0,
        LoRaBandwidth::BW15_6 => 15600.0,
        LoRaBandwidth::BW20_8 => 20800.0,
        LoRaBandwidth::BW31_2 => 31250.0,
        LoRaBandwidth::BW41_7 => 41700.0,
        LoRaBandwidth::BW62_5 => 62500.0,
        LoRaBandwidth::BW125 => 125000.0,
        LoRaBandwidth::BW250 => 250000.0,
        LoRaBandwidth::BW500 => 500000.0,
    };
    let cr_value = match cr {
        CodingRate::CR4_5 => 1,
        CodingRate::CR4_6 => 2,
        CodingRate::CR4_7 => 3,
        CodingRate::CR4_8 => 4,
    };
    // Formula from AN1200.22: BR = SF * (BW / 2^SF) * (4 / (4 + CR))
    let symbol_rate = bw_hz / (2_f64.powf(sf_num as f64));
    let coding_rate_factor = 4.0 / (4.0 + cr_value as f64);
    (sf_num as f64 * symbol_rate * coding_rate_factor).round()
}

/// Calculate Class A RX window delays (Table 13-79, approximate)
pub fn class_a_window_delay_sf(sf: SpreadingFactor) -> (std::time::Duration, std::time::Duration) {
    let delay1_ms = match sf {
        SpreadingFactor::SF5 => 100,
        SpreadingFactor::SF6 => 100,
        SpreadingFactor::SF7 => 100,
        SpreadingFactor::SF8 => 100,
        SpreadingFactor::SF9 => 100,
        SpreadingFactor::SF10 => 100,
        SpreadingFactor::SF11 => 100,
        SpreadingFactor::SF12 => 100, // Simplified; use exact from datasheet for production
    };
    let delay2_ms = 1000; // Fixed 1s for Window 2
    (
        std::time::Duration::from_millis(delay1_ms),
        std::time::Duration::from_millis(delay2_ms),
    )
}

// Extension methods for LoRaModParams
pub trait LoRaModParamsExt {
    fn eu868_defaults() -> Self;
    fn us915_defaults() -> Self;
    fn as923_defaults() -> Self;
    fn validate(&self) -> Result<(), &'static str>;
}

impl LoRaModParamsExt for crate::wmbus::radio::modulation::LoRaModParams {
    /// EU868-optimized defaults for duty cycle compliance
    /// Uses narrower bandwidth and higher SF for better range within 1% duty cycle limits
    fn eu868_defaults() -> Self {
        Self {
            sf: SpreadingFactor::SF9,
            bw: LoRaBandwidth::BW125,  // Standard EU868 bandwidth
            cr: CodingRate::CR4_5,
            low_data_rate_optimize: false,
        }
    }

    /// US915-optimized defaults for high throughput
    /// No duty cycle restrictions, can use wider bandwidth
    fn us915_defaults() -> Self {
        Self {
            sf: SpreadingFactor::SF7,
            bw: LoRaBandwidth::BW500,
            cr: CodingRate::CR4_5,
            low_data_rate_optimize: false,
        }
    }

    /// AS923-optimized defaults (Asia-Pacific)
    fn as923_defaults() -> Self {
        Self {
            sf: SpreadingFactor::SF8,
            bw: LoRaBandwidth::BW125,
            cr: CodingRate::CR4_5,
            low_data_rate_optimize: false,
        }
    }

    /// Validates parameter compatibility and warns about suboptimal combinations
    fn validate(&self) -> Result<(), &'static str> {
        // Check for incompatible SF/BW combinations
        if matches!(self.sf, SpreadingFactor::SF11 | SpreadingFactor::SF12)
            && matches!(self.bw, LoRaBandwidth::BW500) {
            return Err("SF11/SF12 with BW500 not recommended - excessive time on air");
        }

        // Warn about very low data rates
        if matches!(self.sf, SpreadingFactor::SF12)
            && matches!(self.bw, LoRaBandwidth::BW7_8 | LoRaBandwidth::BW10_4) {
            return Err("SF12 with BW<31.2kHz results in <20bps - consider higher BW");
        }

        Ok(())
    }
}

impl Default for LoRaModParams {
    /// Default configuration from SX126x Development Kit User Guide Fig. 9
    /// Optimized for high data rate testing and quick prototyping.
    /// Note: High bandwidth (500kHz) prioritizes speed over range.
    /// For production deployments, consider regional defaults (eu868_defaults, us915_defaults).
    fn default() -> Self {
        Self {
            sf: SpreadingFactor::SF7,      // High data rate
            bw: LoRaBandwidth::BW500,      // Maximum bandwidth for speed
            cr: CodingRate::CR4_5,         // Minimal error correction overhead
            low_data_rate_optimize: false, // Not needed for SF7
        }
    }
}

impl Default for LoRaPacketParams {
    /// Default packet configuration from SX126x Development Kit User Guide
    /// Uses explicit header and CRC for reliability
    fn default() -> Self {
        Self {
            preamble_len: 8,        // Standard preamble (User Guide Fig. 9)
            implicit_header: false,  // Explicit header for flexibility
            payload_len: 64,        // Typical metering packet size
            crc_on: true,           // Enable CRC for data integrity
            iq_inverted: false,     // Standard IQ polarity
        }
    }
}

/// Get LoRa sensitivity in dBm based on SF and BW
/// From AN1200.22 Table 3: Typical sensitivity values for SX126x
pub fn get_lora_sensitivity_dbm(sf: SpreadingFactor, bw: LoRaBandwidth) -> i16 {
    // Sensitivity values from AN1200.22 for SX1262 at 125kHz BW
    // Adjust for bandwidth: +3dB per BW doubling (higher BW = less sensitivity)
    let base_sensitivity_125khz = match sf {
        SpreadingFactor::SF5 => -124,
        SpreadingFactor::SF6 => -127,
        SpreadingFactor::SF7 => -130,
        SpreadingFactor::SF8 => -133,
        SpreadingFactor::SF9 => -136,
        SpreadingFactor::SF10 => -139,
        SpreadingFactor::SF11 => -141,
        SpreadingFactor::SF12 => -144,
    };

    // Bandwidth adjustment (relative to 125kHz)
    let bw_adjustment = match bw {
        LoRaBandwidth::BW7_8 => -6,   // Better sensitivity at lower BW
        LoRaBandwidth::BW10_4 => -5,
        LoRaBandwidth::BW15_6 => -4,
        LoRaBandwidth::BW20_8 => -3,
        LoRaBandwidth::BW31_2 => -2,
        LoRaBandwidth::BW41_7 => -1,
        LoRaBandwidth::BW62_5 => -1,
        LoRaBandwidth::BW125 => 0,    // Reference
        LoRaBandwidth::BW250 => 3,    // Worse sensitivity at higher BW
        LoRaBandwidth::BW500 => 6,
    };

    base_sensitivity_125khz + bw_adjustment
}

/// Get minimum SNR required for demodulation
/// From AN1200.22: SNR floor values for each SF
pub fn get_min_snr_db(sf: SpreadingFactor) -> f32 {
    match sf {
        SpreadingFactor::SF5 => -5.0,
        SpreadingFactor::SF6 => -7.5,
        SpreadingFactor::SF7 => -7.5,
        SpreadingFactor::SF8 => -10.0,
        SpreadingFactor::SF9 => -12.5,
        SpreadingFactor::SF10 => -15.0,
        SpreadingFactor::SF11 => -17.5,
        SpreadingFactor::SF12 => -20.0,
    }
}

/// Determine if LDRO should be enabled based on SF and BW
/// Per AN1200.22: Required for SF11/SF12 when BW <= 125kHz
pub fn requires_ldro(sf: SpreadingFactor, bw: LoRaBandwidth) -> bool {
    matches!(sf, SpreadingFactor::SF11 | SpreadingFactor::SF12)
        && matches!(bw,
            LoRaBandwidth::BW7_8 | LoRaBandwidth::BW10_4 |
            LoRaBandwidth::BW15_6 | LoRaBandwidth::BW20_8 |
            LoRaBandwidth::BW31_2 | LoRaBandwidth::BW41_7 |
            LoRaBandwidth::BW62_5 | LoRaBandwidth::BW125)
}

/// Sync word definitions for network types
pub struct SyncWords;

impl SyncWords {
    /// LoRaWAN public network sync word
    pub const PUBLIC: [u8; 2] = [0x34, 0x44];

    /// Private network sync word (non-LoRaWAN)
    pub const PRIVATE: [u8; 2] = [0x14, 0x24];

    /// Custom sync word for isolated networks
    pub const CUSTOM: [u8; 2] = [0x12, 0x34];
}