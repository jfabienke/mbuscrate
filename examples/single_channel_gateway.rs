//! Single-Channel LoRa Gateway Configuration Example
//!
//! Based on AN1200.94: LoRaWAN Theory for the One-Channel Hub
//!
//! This example demonstrates configuration patterns for fixed-frequency,
//! fixed-SF LoRa gateways suitable for private networks or simple metering deployments.
//!
//! IMPORTANT: For LoRaWAN networks, disable ADR on your network server
//! to prevent SF mismatches. End-devices must be configured to use the
//! same fixed parameters.
//!
//! Usage:
//! ```bash
//! cargo run --example single_channel_gateway
//! ```

use env_logger;
use log::{info, warn};

// Import LoRa types
use mbus_rs::wmbus::radio::lora::{
    LoRaCadParams, LoRaModParams, SpreadingFactor, LoRaBandwidth, CodingRate,
    params::LoRaModParamsExt,
};

/// Single-channel gateway configuration
struct GatewayConfig {
    /// Fixed frequency in Hz
    frequency_hz: u32,
    /// Fixed spreading factor
    spreading_factor: SpreadingFactor,
    /// Fixed bandwidth
    bandwidth: LoRaBandwidth,
    /// Coding rate
    coding_rate: CodingRate,
    /// Transmit power in dBm
    tx_power_dbm: i8,
    /// Maximum duty cycle percentage (regulatory)
    max_duty_cycle_percent: f32,
    /// Use private sync word (non-LoRaWAN)
    use_private_sync_word: bool,
    /// Optimize for reliability over speed
    optimize_for_reliability: bool,
}

impl GatewayConfig {
    /// EU868 configuration - complies with ETSI duty cycle limits
    pub fn eu868() -> Self {
        Self {
            frequency_hz: 868_100_000,  // 868.1 MHz - EU868 channel 1
            spreading_factor: SpreadingFactor::SF9,
            bandwidth: LoRaBandwidth::BW125,
            coding_rate: CodingRate::CR4_5,
            tx_power_dbm: 14,  // ETSI limit
            max_duty_cycle_percent: 1.0,  // 1% duty cycle
            use_private_sync_word: false,
            optimize_for_reliability: true,
        }
    }

    /// US915 configuration - no duty cycle restrictions
    pub fn us915() -> Self {
        Self {
            frequency_hz: 902_300_000,  // US915 channel 0
            spreading_factor: SpreadingFactor::SF7,
            bandwidth: LoRaBandwidth::BW500,
            coding_rate: CodingRate::CR4_5,
            tx_power_dbm: 20,  // FCC limit for US915
            max_duty_cycle_percent: 100.0,  // No duty cycle limit
            use_private_sync_word: false,
            optimize_for_reliability: false,
        }
    }

    /// AS923 configuration - Asia-Pacific region
    pub fn as923() -> Self {
        Self {
            frequency_hz: 923_200_000,  // AS923 channel 1
            spreading_factor: SpreadingFactor::SF8,
            bandwidth: LoRaBandwidth::BW125,
            coding_rate: CodingRate::CR4_5,
            tx_power_dbm: 16,
            max_duty_cycle_percent: 1.0,
            use_private_sync_word: false,
            optimize_for_reliability: true,
        }
    }

    /// Private network configuration for metering
    pub fn metering_network() -> Self {
        Self {
            frequency_hz: 869_525_000,  // Non-standard frequency for private network
            spreading_factor: SpreadingFactor::SF10,  // Good range/data rate balance
            bandwidth: LoRaBandwidth::BW125,
            coding_rate: CodingRate::CR4_6,  // Extra error correction
            tx_power_dbm: 14,
            max_duty_cycle_percent: 1.0,
            use_private_sync_word: true,  // Private sync word to avoid LoRaWAN
            optimize_for_reliability: true,
        }
    }
}

fn main() {
    // Initialize logging
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

    info!("Single-Channel LoRa Gateway Configuration Example");
    info!("Based on AN1200.94: LoRaWAN Theory for the One-Channel Hub");
    info!("");

    // Demonstrate different regional configurations
    info!("Regional Configurations:");
    info!("========================");

    let eu868 = GatewayConfig::eu868();
    info!("EU868: {:.1} MHz, SF{:?}, BW{:?}, {}dBm, {:.1}% duty cycle",
        eu868.frequency_hz as f64 / 1_000_000.0,
        eu868.spreading_factor,
        eu868.bandwidth,
        eu868.tx_power_dbm,
        eu868.max_duty_cycle_percent
    );

    let us915 = GatewayConfig::us915();
    info!("US915: {:.1} MHz, SF{:?}, BW{:?}, {}dBm, {:.1}% duty cycle",
        us915.frequency_hz as f64 / 1_000_000.0,
        us915.spreading_factor,
        us915.bandwidth,
        us915.tx_power_dbm,
        us915.max_duty_cycle_percent
    );

    let as923 = GatewayConfig::as923();
    info!("AS923: {:.1} MHz, SF{:?}, BW{:?}, {}dBm, {:.1}% duty cycle",
        as923.frequency_hz as f64 / 1_000_000.0,
        as923.spreading_factor,
        as923.bandwidth,
        as923.tx_power_dbm,
        as923.max_duty_cycle_percent
    );

    let metering = GatewayConfig::metering_network();
    info!("Private Metering: {:.1} MHz, SF{:?}, BW{:?}, {}dBm, private sync",
        metering.frequency_hz as f64 / 1_000_000.0,
        metering.spreading_factor,
        metering.bandwidth,
        metering.tx_power_dbm
    );

    info!("");
    info!("Key Features Demonstrated:");
    info!("- Fixed frequency/SF operation (no ADR)");
    info!("- CAD (Channel Activity Detection) for LBT");
    info!("- Duty cycle management for regulatory compliance");
    info!("- Regional parameter optimization");
    info!("- Private sync word for non-LoRaWAN networks");
    info!("");
    info!("For actual deployment:");
    info!("1. Implement HAL for your hardware platform");
    info!("2. Add packet forwarding to network server");
    info!("3. Configure end-devices with matching parameters");
    info!("4. Disable ADR on LoRaWAN network server");

    // Demonstrate parameter validation
    let test_params = LoRaModParams {
        sf: SpreadingFactor::SF12,
        bw: LoRaBandwidth::BW500,
        cr: CodingRate::CR4_5,
        low_data_rate_optimize: false,
    };

    info!("");
    info!("Parameter Validation Example:");
    match test_params.validate() {
        Ok(_) => info!("✓ Configuration valid"),
        Err(e) => warn!("✗ Invalid configuration: {}", e),
    }

    // Show CAD timing estimates
    info!("");
    info!("CAD Duration Estimates:");

    let cad_sf7_125 = LoRaCadParams::optimal(SpreadingFactor::SF7, LoRaBandwidth::BW125);
    info!("SF7/BW125: ~{}ms per CAD cycle",
        cad_sf7_125.duration_ms(SpreadingFactor::SF7, LoRaBandwidth::BW125));

    let cad_sf10_125 = LoRaCadParams::optimal(SpreadingFactor::SF10, LoRaBandwidth::BW125);
    info!("SF10/BW125: ~{}ms per CAD cycle",
        cad_sf10_125.duration_ms(SpreadingFactor::SF10, LoRaBandwidth::BW125));

    let cad_sf12_125 = LoRaCadParams::optimal(SpreadingFactor::SF12, LoRaBandwidth::BW125);
    info!("SF12/BW125: ~{}ms per CAD cycle",
        cad_sf12_125.duration_ms(SpreadingFactor::SF12, LoRaBandwidth::BW125));

    info!("");
    info!("This example demonstrates configuration only.");
    info!("For a working gateway, integrate with your hardware HAL.");
}