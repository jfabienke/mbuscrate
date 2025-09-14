//! Adaptive Data Rate (ADR) for LoRa
//!
//! Implements dynamic spreading factor and power adjustment based on signal quality
//! metrics (RSSI/SNR). This optimizes for both range and power consumption while
//! maintaining reliable communication.
//!
//! Based on LoRaWAN ADR algorithms with enhancements from field experience.

use crate::wmbus::radio::modulation::{
    CodingRate, LoRaBandwidth, LoRaModParams, SpreadingFactor,
};
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use std::time::{Duration, Instant};
use log::{debug, info, warn};

/// ADR configuration parameters
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AdrConfig {
    /// Enable ADR functionality
    pub enabled: bool,

    /// Minimum spreading factor allowed
    pub min_sf: SpreadingFactor,

    /// Maximum spreading factor allowed
    pub max_sf: SpreadingFactor,

    /// Minimum transmit power in dBm
    pub min_tx_power: i8,

    /// Maximum transmit power in dBm
    pub max_tx_power: i8,

    /// Number of samples to average for decision making
    pub averaging_window: usize,

    /// Time between ADR evaluations
    pub evaluation_interval: Duration,

    /// RSSI thresholds for SF selection (dBm)
    pub rssi_thresholds: RssiThresholds,

    /// SNR thresholds for SF selection (dB)
    pub snr_thresholds: SnrThresholds,

    /// Hysteresis to prevent oscillation
    pub hysteresis_db: f32,
}

/// RSSI thresholds for spreading factor selection
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RssiThresholds {
    /// RSSI above this → SF7 (best signal)
    pub sf7_threshold: i16,  // Typically -80 dBm

    /// RSSI above this → SF8
    pub sf8_threshold: i16,  // Typically -85 dBm

    /// RSSI above this → SF9
    pub sf9_threshold: i16,  // Typically -90 dBm

    /// RSSI above this → SF10
    pub sf10_threshold: i16, // Typically -95 dBm

    /// RSSI above this → SF11
    pub sf11_threshold: i16, // Typically -100 dBm
    // RSSI below sf11_threshold → SF12 (worst signal)
}

/// SNR thresholds for spreading factor selection
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SnrThresholds {
    /// SNR above this → can use SF7
    pub sf7_min_snr: f32,  // Typically 5 dB

    /// SNR above this → can use SF8
    pub sf8_min_snr: f32,  // Typically 2.5 dB

    /// SNR above this → can use SF9
    pub sf9_min_snr: f32,  // Typically 0 dB

    /// SNR above this → can use SF10
    pub sf10_min_snr: f32, // Typically -2.5 dB

    /// SNR above this → can use SF11
    pub sf11_min_snr: f32, // Typically -5 dB

    /// SNR above this → can use SF12
    pub sf12_min_snr: f32, // Typically -7.5 dB
}

impl Default for AdrConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            min_sf: SpreadingFactor::SF7,
            max_sf: SpreadingFactor::SF12,
            min_tx_power: 2,   // 2 dBm minimum
            max_tx_power: 14,  // 14 dBm for EU868
            averaging_window: 20,
            evaluation_interval: Duration::from_secs(30),
            rssi_thresholds: RssiThresholds {
                sf7_threshold: -80,
                sf8_threshold: -85,
                sf9_threshold: -90,
                sf10_threshold: -95,
                sf11_threshold: -100,
            },
            snr_thresholds: SnrThresholds {
                sf7_min_snr: 5.0,
                sf8_min_snr: 2.5,
                sf9_min_snr: 0.0,
                sf10_min_snr: -2.5,
                sf11_min_snr: -5.0,
                sf12_min_snr: -7.5,
            },
            hysteresis_db: 3.0,
        }
    }
}

/// Signal quality metrics for ADR decisions
#[derive(Debug, Clone, Copy)]
pub struct SignalMetrics {
    /// Received Signal Strength Indicator in dBm
    pub rssi: i16,

    /// Signal-to-Noise Ratio in dB
    pub snr: f32,

    /// Packet Reception Rate (0.0 to 1.0)
    pub prr: f32,

    /// Timestamp of measurement
    pub timestamp: Instant,
}

/// ADR decision output
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AdrDecision {
    /// Recommended spreading factor
    pub spreading_factor: SpreadingFactor,

    /// Recommended transmit power in dBm
    pub tx_power: i8,

    /// Reason for the decision
    pub reason: AdrReason,
}

/// Reason for ADR decision
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum AdrReason {
    /// Initial configuration
    Initial,

    /// Signal improved, reducing SF
    SignalImproved,

    /// Signal degraded, increasing SF
    SignalDegraded,

    /// No change needed
    Stable,

    /// Forced by packet loss
    PacketLoss,

    /// Limited by configuration
    ConfigLimit,
}

/// Adaptive Data Rate controller
pub struct AdrController {
    /// Configuration parameters
    config: AdrConfig,

    /// Current spreading factor
    current_sf: SpreadingFactor,

    /// Current transmit power
    current_tx_power: i8,

    /// Signal metrics history
    metrics_history: VecDeque<SignalMetrics>,

    /// Last evaluation time
    last_evaluation: Instant,

    /// Number of consecutive packet losses
    consecutive_losses: u32,

    /// Last decision made
    last_decision: Option<AdrDecision>,
}

impl Default for AdrController {
    fn default() -> Self {
        Self::new()
    }
}

impl AdrController {
    /// Create a new ADR controller with default configuration
    pub fn new() -> Self {
        Self::with_config(AdrConfig::default())
    }

    /// Create a new ADR controller with custom configuration
    pub fn with_config(config: AdrConfig) -> Self {
        Self {
            current_sf: config.min_sf,
            current_tx_power: config.max_tx_power,
            metrics_history: VecDeque::with_capacity(config.averaging_window),
            last_evaluation: Instant::now(),
            consecutive_losses: 0,
            last_decision: None,
            config,
        }
    }

    /// Record a successful packet reception
    pub fn record_packet(&mut self, rssi: i16, snr: f32) {
        let metrics = SignalMetrics {
            rssi,
            snr,
            prr: self.calculate_prr(),
            timestamp: Instant::now(),
        };

        self.metrics_history.push_back(metrics);

        // Maintain window size
        while self.metrics_history.len() > self.config.averaging_window {
            self.metrics_history.pop_front();
        }

        // Reset loss counter on successful reception
        self.consecutive_losses = 0;

        debug!("ADR: Recorded packet - RSSI: {rssi} dBm, SNR: {snr} dB");
    }

    /// Record a packet loss
    pub fn record_loss(&mut self) {
        self.consecutive_losses += 1;

        warn!("ADR: Packet loss recorded (consecutive: {})", self.consecutive_losses);

        // Force evaluation if too many losses
        if self.consecutive_losses >= 3 {
            self.force_evaluation();
        }
    }

    /// Force an ADR evaluation immediately
    pub fn force_evaluation(&mut self) -> AdrDecision {
        self.last_evaluation = Instant::now();
        self.evaluate_internal()
    }

    /// Evaluate ADR and return decision if it's time
    pub fn evaluate(&mut self) -> Option<AdrDecision> {
        if !self.config.enabled {
            return None;
        }

        // Check if it's time to evaluate
        if self.last_evaluation.elapsed() < self.config.evaluation_interval {
            return None;
        }

        // Need minimum samples for decision
        if self.metrics_history.len() < 5 {
            return None;
        }

        self.last_evaluation = Instant::now();
        let decision = self.evaluate_internal();

        if decision.reason != AdrReason::Stable {
            Some(decision)
        } else {
            None
        }
    }

    /// Internal evaluation logic
    fn evaluate_internal(&mut self) -> AdrDecision {
        // Handle packet loss scenario
        if self.consecutive_losses >= 3 {
            return self.handle_packet_loss();
        }

        // Calculate average metrics
        let (avg_rssi, avg_snr) = self.calculate_averages();

        // Determine optimal SF based on signal quality
        let optimal_sf = self.determine_optimal_sf(avg_rssi, avg_snr);

        // Apply hysteresis to prevent oscillation
        let target_sf = self.apply_hysteresis(optimal_sf, avg_rssi);

        // Determine power adjustment
        let target_power = self.determine_tx_power(target_sf, avg_rssi);

        // Create decision
        let reason = if target_sf < self.current_sf {
            AdrReason::SignalImproved
        } else if target_sf > self.current_sf {
            AdrReason::SignalDegraded
        } else {
            AdrReason::Stable
        };

        // Update current settings
        if reason != AdrReason::Stable {
            info!(
                "ADR: Changing SF{} → SF{}, Power: {} dBm (RSSI: {} dBm, SNR: {} dB)",
                self.current_sf as u8,
                target_sf as u8,
                target_power,
                avg_rssi,
                avg_snr
            );

            self.current_sf = target_sf;
            self.current_tx_power = target_power;
        }

        let decision = AdrDecision {
            spreading_factor: target_sf,
            tx_power: target_power,
            reason,
        };

        self.last_decision = Some(decision);
        decision
    }

    /// Handle packet loss by increasing robustness
    fn handle_packet_loss(&mut self) -> AdrDecision {
        // Increase SF for better sensitivity
        let new_sf = match self.current_sf {
            SpreadingFactor::SF7 => SpreadingFactor::SF8,
            SpreadingFactor::SF8 => SpreadingFactor::SF9,
            SpreadingFactor::SF9 => SpreadingFactor::SF10,
            SpreadingFactor::SF10 => SpreadingFactor::SF11,
            SpreadingFactor::SF11 => SpreadingFactor::SF12,
            SpreadingFactor::SF12 => SpreadingFactor::SF12, // Already at max
            _ => self.current_sf,
        };

        // Increase power if not at max
        let new_power = (self.current_tx_power + 2).min(self.config.max_tx_power);

        warn!(
            "ADR: Packet loss mitigation - SF{} → SF{}, Power: {} → {} dBm",
            self.current_sf as u8, new_sf as u8, self.current_tx_power, new_power
        );

        self.current_sf = new_sf;
        self.current_tx_power = new_power;
        self.consecutive_losses = 0;  // Reset counter

        AdrDecision {
            spreading_factor: new_sf,
            tx_power: new_power,
            reason: AdrReason::PacketLoss,
        }
    }

    /// Calculate average RSSI and SNR from history
    fn calculate_averages(&self) -> (i16, f32) {
        if self.metrics_history.is_empty() {
            return (-100, 0.0);
        }

        let sum_rssi: i32 = self.metrics_history.iter().map(|m| m.rssi as i32).sum();
        let sum_snr: f32 = self.metrics_history.iter().map(|m| m.snr).sum();

        let count = self.metrics_history.len();
        let avg_rssi = (sum_rssi / count as i32) as i16;
        let avg_snr = sum_snr / count as f32;

        (avg_rssi, avg_snr)
    }

    /// Calculate packet reception rate
    fn calculate_prr(&self) -> f32 {
        if self.consecutive_losses == 0 {
            1.0
        } else {
            let total = self.metrics_history.len() + self.consecutive_losses as usize;
            self.metrics_history.len() as f32 / total as f32
        }
    }

    /// Determine optimal SF based on RSSI and SNR
    fn determine_optimal_sf(&self, rssi: i16, snr: f32) -> SpreadingFactor {
        // First check SNR constraints
        let sf_from_snr = if snr >= self.config.snr_thresholds.sf7_min_snr {
            SpreadingFactor::SF7
        } else if snr >= self.config.snr_thresholds.sf8_min_snr {
            SpreadingFactor::SF8
        } else if snr >= self.config.snr_thresholds.sf9_min_snr {
            SpreadingFactor::SF9
        } else if snr >= self.config.snr_thresholds.sf10_min_snr {
            SpreadingFactor::SF10
        } else if snr >= self.config.snr_thresholds.sf11_min_snr {
            SpreadingFactor::SF11
        } else {
            SpreadingFactor::SF12
        };

        // Then check RSSI constraints
        let sf_from_rssi = if rssi >= self.config.rssi_thresholds.sf7_threshold {
            SpreadingFactor::SF7
        } else if rssi >= self.config.rssi_thresholds.sf8_threshold {
            SpreadingFactor::SF8
        } else if rssi >= self.config.rssi_thresholds.sf9_threshold {
            SpreadingFactor::SF9
        } else if rssi >= self.config.rssi_thresholds.sf10_threshold {
            SpreadingFactor::SF10
        } else if rssi >= self.config.rssi_thresholds.sf11_threshold {
            SpreadingFactor::SF11
        } else {
            SpreadingFactor::SF12
        };

        // Use the more conservative (higher) SF
        std::cmp::max(sf_from_snr, sf_from_rssi)
    }

    /// Apply hysteresis to prevent oscillation
    fn apply_hysteresis(&self, target_sf: SpreadingFactor, rssi: i16) -> SpreadingFactor {
        // Only allow decreasing SF if signal is significantly better
        if target_sf < self.current_sf {
            let threshold = match self.current_sf {
                SpreadingFactor::SF8 => self.config.rssi_thresholds.sf7_threshold,
                SpreadingFactor::SF9 => self.config.rssi_thresholds.sf8_threshold,
                SpreadingFactor::SF10 => self.config.rssi_thresholds.sf9_threshold,
                SpreadingFactor::SF11 => self.config.rssi_thresholds.sf10_threshold,
                SpreadingFactor::SF12 => self.config.rssi_thresholds.sf11_threshold,
                _ => return self.current_sf,
            };

            if rssi > threshold + self.config.hysteresis_db as i16 {
                target_sf
            } else {
                self.current_sf
            }
        } else {
            target_sf
        }
    }

    /// Determine transmit power based on SF and signal margin
    fn determine_tx_power(&self, sf: SpreadingFactor, rssi: i16) -> i8 {
        // Calculate link margin
        let sensitivity = match sf {
            SpreadingFactor::SF7 => -123,
            SpreadingFactor::SF8 => -126,
            SpreadingFactor::SF9 => -129,
            SpreadingFactor::SF10 => -132,
            SpreadingFactor::SF11 => -134,
            SpreadingFactor::SF12 => -137,
            _ => -130,
        };

        let link_margin = rssi - sensitivity;

        // Adjust power to maintain 10dB margin
        let target_margin = 10;
        let power_adjustment = target_margin - link_margin;

        

        (self.current_tx_power + power_adjustment as i8)
            .max(self.config.min_tx_power)
            .min(self.config.max_tx_power)
    }

    /// Get current ADR state
    pub fn get_current_state(&self) -> (SpreadingFactor, i8) {
        (self.current_sf, self.current_tx_power)
    }

    /// Apply ADR decision from network (LinkADRReq)
    pub fn apply_network_adr(&mut self, sf: SpreadingFactor, tx_power: i8) {
        info!(
            "ADR: Applying network command - SF{}, {} dBm",
            sf as u8, tx_power
        );

        self.current_sf = sf;
        self.current_tx_power = tx_power.max(self.config.min_tx_power)
            .min(self.config.max_tx_power);

        // Clear history to start fresh with new parameters
        self.metrics_history.clear();
        self.consecutive_losses = 0;
    }

    /// Convert current state to LoRa modulation parameters
    pub fn to_mod_params(&self) -> LoRaModParams {
        LoRaModParams {
            sf: self.current_sf,
            bw: LoRaBandwidth::BW125,  // Fixed for now
            cr: CodingRate::CR4_5,     // Fixed for now
            low_data_rate_optimize: matches!(
                self.current_sf,
                SpreadingFactor::SF11 | SpreadingFactor::SF12
            ),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_adr_signal_improvement() {
        let mut adr = AdrController::new();

        // Start at SF12 (worst case)
        adr.current_sf = SpreadingFactor::SF12;

        // Record good signal packets
        for _ in 0..10 {
            adr.record_packet(-75, 10.0);  // Strong signal
        }

        // Force evaluation
        let decision = adr.force_evaluation();

        // Should recommend lower SF
        assert_eq!(decision.spreading_factor, SpreadingFactor::SF7);
        assert_eq!(decision.reason, AdrReason::SignalImproved);
    }

    #[test]
    fn test_adr_packet_loss_handling() {
        let mut adr = AdrController::new();
        adr.current_sf = SpreadingFactor::SF8;

        // First record some packets to build history (required for evaluation)
        for _ in 0..5 {
            adr.record_packet(-90, -2.0); // Marginal signal at SF8
        }

        // Force evaluation interval to pass
        std::thread::sleep(std::time::Duration::from_millis(1));
        adr.last_evaluation = std::time::Instant::now() - std::time::Duration::from_secs(31);

        let initial_sf = adr.current_sf;

        // Record multiple losses (the third one will force evaluation and apply changes)
        adr.record_loss();
        adr.record_loss();
        adr.record_loss(); // This triggers force_evaluation() which applies the decision

        // Check that SF was increased due to packet loss
        assert!(adr.current_sf > initial_sf, "SF should have increased from {:?} to {:?}", initial_sf, adr.current_sf);

        // The internal state should now show the adjustment was applied
        println!("SF changed from {:?} to {:?}", initial_sf, adr.current_sf);
    }

    #[test]
    fn test_adr_hysteresis() {
        let config = AdrConfig {
            hysteresis_db: 3.0,
            ..Default::default()
        };

        let mut adr = AdrController::with_config(config);
        adr.current_sf = SpreadingFactor::SF8;

        // Record signal just slightly better than SF7 threshold
        for _ in 0..10 {
            adr.record_packet(-79, 8.0);  // Just above -80 dBm threshold
        }

        let decision = adr.force_evaluation();

        // Should not change due to hysteresis
        assert_eq!(decision.spreading_factor, SpreadingFactor::SF8);
        assert_eq!(decision.reason, AdrReason::Stable);
    }
}