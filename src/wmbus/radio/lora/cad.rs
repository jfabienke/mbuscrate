//! Channel Activity Detection (CAD) support for LoRa
//!
//! Implements optimized CAD parameters based on AN1200.48 for robust
//! Listen Before Talk (LBT) functionality. CAD provides faster and more
//! accurate LoRa signal detection compared to RSSI-based methods.

use crate::wmbus::radio::modulation::{SpreadingFactor, LoRaBandwidth};
use serde::{Deserialize, Serialize};

/// CAD exit modes determining radio behavior after detection
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CadExitMode {
    /// Return to standby after CAD (fast, low power)
    CadOnly = 0x00,
    /// Switch to RX if activity detected (automatic reception)
    CadToRx = 0x01,
}

/// Channel Activity Detection parameters
///
/// Optimized values from AN1200.48 for different SF/BW combinations
/// to minimize false positives while maintaining sensitivity.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct LoRaCadParams {
    /// Number of symbols for CAD (1, 2, 4, 8, or 16)
    /// Higher values improve accuracy but increase detection time
    pub symbol_num: u8,

    /// Peak detection threshold (18-30)
    /// Higher values reduce false positives but may miss weak signals
    pub det_peak: u8,

    /// Minimum detection threshold (8-15)
    /// Sets the noise floor for detection
    pub det_min: u8,

    /// Behavior after CAD completion
    pub exit_mode: CadExitMode,
}

impl Default for LoRaCadParams {
    /// Default CAD parameters for SF7/BW125 (most common configuration)
    fn default() -> Self {
        Self {
            symbol_num: 2,
            det_peak: 22,
            det_min: 10,
            exit_mode: CadExitMode::CadOnly,
        }
    }
}

impl LoRaCadParams {
    /// Returns optimal CAD parameters based on AN1200.48 empirical data
    ///
    /// These values have been tested by Semtech to provide the best
    /// trade-off between detection accuracy and false positive rate.
    ///
    /// # Arguments
    ///
    /// * `sf` - Spreading Factor
    /// * `bw` - Bandwidth
    ///
    /// # Returns
    ///
    /// Optimized CAD parameters for the given SF/BW combination
    pub fn optimal(sf: SpreadingFactor, bw: LoRaBandwidth) -> Self {
        let (symbol_num, det_peak, det_min) = match bw {
            // Table 1: Optimal CAD parameters for BW125 (AN1200.48)
            LoRaBandwidth::BW125 => match sf {
                SpreadingFactor::SF5 | SpreadingFactor::SF6 => (2, 22, 10),
                SpreadingFactor::SF7 | SpreadingFactor::SF8 | SpreadingFactor::SF9 => (2, 22, 10),
                SpreadingFactor::SF10 | SpreadingFactor::SF11 => (4, 21, 10),
                SpreadingFactor::SF12 => (8, 20, 10),
            },

            // Table 43: Optimal CAD parameters for BW500 (AN1200.48)
            LoRaBandwidth::BW500 => match sf {
                SpreadingFactor::SF5 | SpreadingFactor::SF6 => (4, 22, 10),
                SpreadingFactor::SF7 | SpreadingFactor::SF8 | SpreadingFactor::SF9 => (4, 21, 10),
                SpreadingFactor::SF10 | SpreadingFactor::SF11 => (8, 20, 10),
                SpreadingFactor::SF12 => (16, 19, 10),
            },

            // Conservative defaults for other bandwidths
            LoRaBandwidth::BW250 => match sf {
                SpreadingFactor::SF5 | SpreadingFactor::SF6 | SpreadingFactor::SF7 => (2, 22, 10),
                SpreadingFactor::SF8 | SpreadingFactor::SF9 => (4, 21, 10),
                SpreadingFactor::SF10 | SpreadingFactor::SF11 => (4, 21, 10),
                SpreadingFactor::SF12 => (8, 20, 10),
            },

            // Narrow bandwidths: More symbols needed for accuracy
            LoRaBandwidth::BW62_5 | LoRaBandwidth::BW41_7 | LoRaBandwidth::BW31_2 => match sf {
                SpreadingFactor::SF5 | SpreadingFactor::SF6 | SpreadingFactor::SF7 => (4, 22, 10),
                SpreadingFactor::SF8 | SpreadingFactor::SF9 => (4, 22, 10),
                SpreadingFactor::SF10 | SpreadingFactor::SF11 => (8, 21, 10),
                SpreadingFactor::SF12 => (16, 20, 10),
            },

            // Very narrow bandwidths: Maximum symbols for reliability
            LoRaBandwidth::BW20_8 | LoRaBandwidth::BW15_6 | LoRaBandwidth::BW10_4 | LoRaBandwidth::BW7_8 => {
                match sf {
                    SpreadingFactor::SF5 | SpreadingFactor::SF6 | SpreadingFactor::SF7 => (8, 22, 10),
                    SpreadingFactor::SF8 | SpreadingFactor::SF9 => (8, 22, 10),
                    SpreadingFactor::SF10 | SpreadingFactor::SF11 => (16, 21, 10),
                    SpreadingFactor::SF12 => (16, 20, 10),
                }
            }
        };

        Self {
            symbol_num,
            det_peak,
            det_min,
            exit_mode: CadExitMode::CadOnly,
        }
    }

    /// Creates CAD parameters optimized for fast detection
    ///
    /// Uses minimum symbol count for quickest detection at the cost
    /// of potentially higher false positive rate.
    pub fn fast_detect(sf: SpreadingFactor, bw: LoRaBandwidth) -> Self {
        let mut params = Self::optimal(sf, bw);
        params.symbol_num = params.symbol_num.min(2); // Use at most 2 symbols
        params.det_peak = params.det_peak.saturating_add(1); // Slightly stricter threshold
        params
    }

    /// Creates CAD parameters optimized for high reliability
    ///
    /// Uses more symbols and stricter thresholds to minimize false
    /// positives, suitable for critical applications.
    pub fn high_reliability(sf: SpreadingFactor, bw: LoRaBandwidth) -> Self {
        let mut params = Self::optimal(sf, bw);
        params.symbol_num = (params.symbol_num * 2).min(16); // Double symbols (max 16)
        params.det_min = params.det_min.saturating_add(2); // Raise noise floor
        params
    }

    /// Calculates approximate CAD duration in milliseconds
    ///
    /// Based on symbol time for the given SF/BW combination.
    pub fn duration_ms(&self, sf: SpreadingFactor, bw: LoRaBandwidth) -> u32 {
        // Symbol time in ms: T_sym = 2^SF / BW * 1000
        let bw_hz = match bw {
            LoRaBandwidth::BW7_8 => 7_800,
            LoRaBandwidth::BW10_4 => 10_400,
            LoRaBandwidth::BW15_6 => 15_600,
            LoRaBandwidth::BW20_8 => 20_800,
            LoRaBandwidth::BW31_2 => 31_250,
            LoRaBandwidth::BW41_7 => 41_700,
            LoRaBandwidth::BW62_5 => 62_500,
            LoRaBandwidth::BW125 => 125_000,
            LoRaBandwidth::BW250 => 250_000,
            LoRaBandwidth::BW500 => 500_000,
        };

        let sf_val = match sf {
            SpreadingFactor::SF5 => 5,
            SpreadingFactor::SF6 => 6,
            SpreadingFactor::SF7 => 7,
            SpreadingFactor::SF8 => 8,
            SpreadingFactor::SF9 => 9,
            SpreadingFactor::SF10 => 10,
            SpreadingFactor::SF11 => 11,
            SpreadingFactor::SF12 => 12,
        };

        // CAD duration = symbol_num * T_sym
        // Use floating point for accuracy, ensure minimum 1ms
        let symbol_time_ms = (1 << sf_val) as f32 * 1000.0 / bw_hz as f32;
        let duration = self.symbol_num as f32 * symbol_time_ms;
        duration.ceil() as u32
    }
}

/// CAD detection statistics for monitoring and optimization
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CadStats {
    /// Total number of CAD operations performed
    pub total_cad_operations: u32,

    /// Number of times activity was detected
    pub activity_detected: u32,

    /// Number of times channel was clear
    pub channel_clear: u32,

    /// Number of CAD timeouts
    pub timeouts: u32,

    /// Average CAD duration in milliseconds
    pub avg_duration_ms: f32,

    /// False positive rate (if known from testing)
    pub false_positive_rate: Option<f32>,
}

impl CadStats {
    /// Updates statistics after a CAD operation
    pub fn record_cad(&mut self, detected: bool, duration_ms: u32) {
        self.total_cad_operations += 1;

        if detected {
            self.activity_detected += 1;
        } else {
            self.channel_clear += 1;
        }

        // Update average duration (running average)
        let n = self.total_cad_operations as f32;
        self.avg_duration_ms =
            (self.avg_duration_ms * (n - 1.0) + duration_ms as f32) / n;
    }

    /// Gets the detection rate (0.0 to 1.0)
    pub fn detection_rate(&self) -> f32 {
        if self.total_cad_operations == 0 {
            0.0
        } else {
            self.activity_detected as f32 / self.total_cad_operations as f32
        }
    }

    /// Resets all statistics
    pub fn reset(&mut self) {
        *self = Self::default();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_optimal_cad_params() {
        // Test BW125 optimal values from AN1200.48 Table 1
        let params_sf7 = LoRaCadParams::optimal(SpreadingFactor::SF7, LoRaBandwidth::BW125);
        assert_eq!(params_sf7.symbol_num, 2);
        assert_eq!(params_sf7.det_peak, 22);
        assert_eq!(params_sf7.det_min, 10);

        let params_sf12 = LoRaCadParams::optimal(SpreadingFactor::SF12, LoRaBandwidth::BW125);
        assert_eq!(params_sf12.symbol_num, 8);
        assert_eq!(params_sf12.det_peak, 20);

        // Test BW500 optimal values from AN1200.48 Table 43
        let params_sf7_bw500 = LoRaCadParams::optimal(SpreadingFactor::SF7, LoRaBandwidth::BW500);
        assert_eq!(params_sf7_bw500.symbol_num, 4);
        assert_eq!(params_sf7_bw500.det_peak, 21);
    }

    #[test]
    fn test_cad_duration() {
        let params = LoRaCadParams::optimal(SpreadingFactor::SF7, LoRaBandwidth::BW125);
        let duration = params.duration_ms(SpreadingFactor::SF7, LoRaBandwidth::BW125);

        // SF7, BW125: T_sym = 128/125000 * 1000 = ~1ms
        // 2 symbols = ~2ms
        assert!(duration >= 1 && duration <= 3);

        let params_sf12 = LoRaCadParams::optimal(SpreadingFactor::SF12, LoRaBandwidth::BW125);
        let duration_sf12 = params_sf12.duration_ms(SpreadingFactor::SF12, LoRaBandwidth::BW125);

        // SF12, BW125: T_sym = 4096/125000 * 1000 = ~33ms
        // 8 symbols = ~264ms
        assert!(duration_sf12 >= 200 && duration_sf12 <= 300);
    }

    #[test]
    fn test_cad_stats() {
        let mut stats = CadStats::default();

        stats.record_cad(true, 10);
        stats.record_cad(false, 12);
        stats.record_cad(true, 8);

        assert_eq!(stats.total_cad_operations, 3);
        assert_eq!(stats.activity_detected, 2);
        assert_eq!(stats.channel_clear, 1);
        assert!((stats.detection_rate() - 0.666).abs() < 0.01);
        assert!((stats.avg_duration_ms - 10.0).abs() < 0.1);
    }
}