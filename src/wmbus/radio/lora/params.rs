
/// LoRa-specific parameters and enums for SX126x configuration.

/// Spreading Factor (SF) for LoRa (Table 13-47)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CodingRate {
    CR4_5 = 0x01,
    CR4_6 = 0x02,
    CR4_7 = 0x03,
    CR4_8 = 0x04,
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

/// LoRa packet parameters
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LoRaPacketParams {
    pub preamble_len: u16,      // 8 to 65535 symbols
    pub implicit_header: bool,  // true for implicit, false for explicit
    pub payload_len: u8,        // For implicit header mode
    pub crc_on: bool,
    pub iq_inverted: bool,
}

/// Calculate LoRa bitrate in bps (datasheet formula)
pub fn lora_bitrate_hz(sf: SpreadingFactor, bw: LoRaBandwidth, cr: CodingRate) -> f64 {
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
    let cr_num = match cr {
        CodingRate::CR4_5 => 5,
        CodingRate::CR4_6 => 6,
        CodingRate::CR4_7 => 7,
        CodingRate::CR4_8 => 8,
    };
    (sf_num as f64 * bw_hz / (2_f64.powf(sf_num as f64)) * 4.0 / (4.0 + cr_num as f64)).round()
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