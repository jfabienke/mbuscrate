//! Single-Channel LoRa Configuration
//!
//! Implements single-channel optimization for LoRa gateways, locking the radio
//! to a specific frequency, spreading factor, and bandwidth. This simplifies
//! operation and improves throughput (500+ packets/hour) while ensuring
//! duty cycle compliance.
//!
//! Inspired by One Channel Hub's approach to single-channel gateways.

use crate::wmbus::radio::modulation::{
    CodingRate, LoRaBandwidth, LoRaModParams, SpreadingFactor,
};
use serde::{Deserialize, Serialize};
use std::time::{Duration, Instant};

/// Single-channel configuration for LoRa radio
///
/// Locks the radio to a specific frequency and modulation parameters
/// for optimized single-channel operation.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct SingleChannelConfig {
    /// Center frequency in Hz (e.g., 868_100_000 for EU868.1)
    pub frequency_hz: u32,

    /// Spreading Factor (SF7-SF12)
    pub spreading_factor: SpreadingFactor,

    /// Bandwidth (typically 125kHz for EU868)
    pub bandwidth: LoRaBandwidth,

    /// Coding Rate (typically 4/5)
    pub coding_rate: CodingRate,

    /// Transmit power in dBm (max 14dBm for EU868)
    pub tx_power_dbm: i8,

    /// Enable duty cycle limiting (required for EU868)
    pub duty_cycle_enabled: bool,

    /// Duty cycle limit as percentage (1% for EU868)
    pub duty_cycle_percent: f32,
}

impl Default for SingleChannelConfig {
    fn default() -> Self {
        // Default to EU868 channel 1 with SF7
        Self {
            frequency_hz: 868_100_000,
            spreading_factor: SpreadingFactor::SF7,
            bandwidth: LoRaBandwidth::BW125,
            coding_rate: CodingRate::CR4_5,
            tx_power_dbm: 14,
            duty_cycle_enabled: true,
            duty_cycle_percent: 1.0,
        }
    }
}

impl SingleChannelConfig {
    /// Create configuration for EU868 channel 1 (most common)
    pub fn eu868_channel_1() -> Self {
        Self::default()
    }

    /// Create configuration for EU868 channel 2
    pub fn eu868_channel_2() -> Self {
        Self {
            frequency_hz: 868_300_000,
            ..Self::default()
        }
    }

    /// Create configuration for EU868 channel 3
    pub fn eu868_channel_3() -> Self {
        Self {
            frequency_hz: 868_500_000,
            ..Self::default()
        }
    }

    /// Create configuration for US915 channel 0
    pub fn us915_channel_0() -> Self {
        Self {
            frequency_hz: 902_300_000,
            spreading_factor: SpreadingFactor::SF7,
            bandwidth: LoRaBandwidth::BW125,
            coding_rate: CodingRate::CR4_5,
            tx_power_dbm: 20,  // Higher power allowed in US
            duty_cycle_enabled: false,  // No duty cycle limit in US
            duty_cycle_percent: 100.0,
        }
    }

    /// Convert to LoRa modulation parameters
    pub fn to_mod_params(&self) -> LoRaModParams {
        LoRaModParams {
            sf: self.spreading_factor,
            bw: self.bandwidth,
            cr: self.coding_rate,
            low_data_rate_optimize: matches!(
                self.spreading_factor,
                SpreadingFactor::SF11 | SpreadingFactor::SF12
            ),
        }
    }

    /// Calculate maximum throughput in packets per hour
    ///
    /// Based on spreading factor, bandwidth, and duty cycle limits
    pub fn max_throughput_per_hour(&self) -> u32 {
        // Time on air calculation (simplified)
        let toa_ms = self.calculate_time_on_air_ms(50);  // Assume 50-byte packet

        // Account for duty cycle
        let effective_duty_cycle = if self.duty_cycle_enabled {
            self.duty_cycle_percent / 100.0
        } else {
            1.0
        };

        // Calculate packets per hour
        let ms_per_hour = 3_600_000.0;
        let available_ms = ms_per_hour * effective_duty_cycle;

        (available_ms / toa_ms as f32) as u32
    }

    /// Calculate time on air for a given payload size
    fn calculate_time_on_air_ms(&self, payload_bytes: usize) -> u32 {
        // Simplified ToA calculation for LoRa
        // Real calculation is more complex and depends on many factors
        let sf = match self.spreading_factor {
            SpreadingFactor::SF7 => 7,
            SpreadingFactor::SF8 => 8,
            SpreadingFactor::SF9 => 9,
            SpreadingFactor::SF10 => 10,
            SpreadingFactor::SF11 => 11,
            SpreadingFactor::SF12 => 12,
            _ => 7,
        };

        let bw_khz = match self.bandwidth {
            LoRaBandwidth::BW125 => 125,
            LoRaBandwidth::BW250 => 250,
            LoRaBandwidth::BW500 => 500,
            _ => 125,
        };

        // Simplified formula (actual is more complex)
        let symbol_time_ms = (1 << sf) as f32 / bw_khz as f32;
        let num_symbols = 8 + (payload_bytes * 8 / sf) as u32;

        (num_symbols as f32 * symbol_time_ms) as u32
    }
}

/// Duty cycle limiter for regulatory compliance
///
/// Tracks transmission time and enforces duty cycle limits
/// as required by regulations (e.g., 1% for EU868).
pub struct DutyCycleLimiter {
    /// Configuration for this limiter
    config: SingleChannelConfig,

    /// Transmission history for duty cycle calculation
    tx_history: Vec<(Instant, Duration)>,

    /// Time window for duty cycle calculation (typically 1 hour)
    window: Duration,
}

impl DutyCycleLimiter {
    /// Create a new duty cycle limiter
    pub fn new(config: SingleChannelConfig) -> Self {
        Self {
            config,
            tx_history: Vec::new(),
            window: Duration::from_secs(3600),  // 1 hour window
        }
    }

    /// Check if transmission is allowed based on duty cycle
    pub fn can_transmit(&mut self, duration: Duration) -> bool {
        if !self.config.duty_cycle_enabled {
            return true;
        }

        self.cleanup_old_entries();

        // Calculate current duty cycle
        let total_tx_time: Duration = self.tx_history
            .iter()
            .map(|(_, d)| *d)
            .sum();

        let window_ms = self.window.as_millis() as f32;
        let total_tx_ms = total_tx_time.as_millis() as f32;
        let projected_tx_ms = total_tx_ms + duration.as_millis() as f32;

        let projected_duty_cycle = (projected_tx_ms / window_ms) * 100.0;

        projected_duty_cycle <= self.config.duty_cycle_percent
    }

    /// Record a transmission for duty cycle tracking
    pub fn record_transmission(&mut self, duration: Duration) {
        self.tx_history.push((Instant::now(), duration));
        self.cleanup_old_entries();
    }

    /// Get current duty cycle as percentage
    pub fn get_current_duty_cycle(&mut self) -> f32 {
        self.cleanup_old_entries();

        let total_tx_time: Duration = self.tx_history
            .iter()
            .map(|(_, d)| *d)
            .sum();

        let window_ms = self.window.as_millis() as f32;
        let total_tx_ms = total_tx_time.as_millis() as f32;

        (total_tx_ms / window_ms) * 100.0
    }

    /// Remove entries older than the time window
    fn cleanup_old_entries(&mut self) {
        let cutoff = Instant::now() - self.window;
        self.tx_history.retain(|(instant, _)| *instant > cutoff);
    }

    /// Get time until next transmission is allowed
    pub fn time_until_available(&mut self) -> Option<Duration> {
        if !self.config.duty_cycle_enabled {
            return None;
        }

        self.cleanup_old_entries();

        // If we're under the limit, transmission is allowed immediately
        if self.get_current_duty_cycle() < self.config.duty_cycle_percent {
            return None;
        }

        // Find the oldest transmission that would expire to bring us under limit
        if let Some((oldest_time, _)) = self.tx_history.first() {
            let time_until_expiry = (*oldest_time + self.window) - Instant::now();
            Some(time_until_expiry)
        } else {
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_single_channel_config_defaults() {
        let config = SingleChannelConfig::default();
        assert_eq!(config.frequency_hz, 868_100_000);
        assert_eq!(config.spreading_factor, SpreadingFactor::SF7);
        assert_eq!(config.duty_cycle_percent, 1.0);
    }

    #[test]
    fn test_max_throughput_calculation() {
        let config = SingleChannelConfig::eu868_channel_1();
        let throughput = config.max_throughput_per_hour();

        // With SF7 and 1% duty cycle, should get ~500-1000 packets/hour
        assert!(throughput > 400);
        assert!(throughput < 2000);
    }

    #[test]
    fn test_duty_cycle_limiter() {
        let config = SingleChannelConfig::eu868_channel_1();
        let mut limiter = DutyCycleLimiter::new(config);

        // Should allow initial transmission
        assert!(limiter.can_transmit(Duration::from_millis(100)));

        // Record a transmission
        limiter.record_transmission(Duration::from_millis(100));

        // Should still be under 1% limit
        assert!(limiter.get_current_duty_cycle() < 1.0);
    }
}