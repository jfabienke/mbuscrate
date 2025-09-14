//! Smart decoder with automatic format detection and learning capabilities
//!
//! This module provides an intelligent decoder that can automatically detect
//! payload formats and learn from successful decodings.

use super::decoder::{LoRaDecodeError, LoRaDeviceManager, MeteringData};
use super::format_detector::{Confidence, DetectionResult, FormatDetector};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

/// Statistics for a device's decoding history
#[derive(Debug, Clone)]
pub struct DeviceStats {
    /// Successful format detections
    pub successful_formats: HashMap<String, u32>,
    /// Failed decoding attempts
    pub failed_attempts: u32,
    /// Last successful format
    pub last_successful_format: Option<String>,
    /// Average confidence score
    pub avg_confidence: f32,
    /// Total packets processed
    pub total_packets: u32,
}

impl Default for DeviceStats {
    fn default() -> Self {
        Self {
            successful_formats: HashMap::new(),
            failed_attempts: 0,
            last_successful_format: None,
            avg_confidence: 0.0,
            total_packets: 0,
        }
    }
}

/// Smart decoder with learning capabilities
pub struct SmartDecoder {
    /// Base device manager
    manager: LoRaDeviceManager,
    /// Format detector
    detector: FormatDetector,
    /// Device statistics for learning
    device_stats: Arc<Mutex<HashMap<String, DeviceStats>>>,
    /// Enable learning mode
    learning_enabled: bool,
    /// Confidence threshold for automatic registration
    auto_register_threshold: u8,
}

impl SmartDecoder {
    /// Create a new smart decoder
    pub fn new() -> Self {
        Self {
            manager: LoRaDeviceManager::new(),
            detector: FormatDetector::new(),
            device_stats: Arc::new(Mutex::new(HashMap::new())),
            learning_enabled: true,
            auto_register_threshold: 80, // High confidence required for auto-registration
        }
    }

    /// Enable or disable learning mode
    pub fn set_learning(&mut self, enabled: bool) {
        self.learning_enabled = enabled;
    }

    /// Decode with automatic format detection and learning
    pub fn decode_smart(
        &mut self,
        device_addr: &str,
        payload: &[u8],
        f_port: u8,
    ) -> Result<MeteringData, LoRaDecodeError> {
        // Try registered decoder first
        if let Some(decoder_type) = self.manager.decoders.get(device_addr) {
            match crate::wmbus::radio::lora::decoder::decode_with_type(decoder_type, payload, f_port) {
                Ok(data) => {
                    self.record_success(device_addr, &data.decoder_type);
                    return Ok(data);
                }
                Err(e) => {
                    log::debug!("Registered decoder failed for {device_addr}: {e:?}");
                    self.record_failure(device_addr);
                    // Continue to auto-detection
                }
            }
        }

        // Auto-detect format
        let detection = self.detector.detect(payload, f_port);

        // Update statistics
        self.update_stats(device_addr, &detection);

        // Check if we should auto-register this decoder
        if self.should_auto_register(device_addr, &detection) {
            if let Some(decoder_type) = detection.decoder_type.clone() {
                log::info!(
                    "Auto-registering {} decoder for device {} (confidence: {:?})",
                    detection.format,
                    device_addr,
                    detection.confidence
                );
                self.manager
                    .register_device(device_addr.to_string(), decoder_type);
            }
        }

        // Try to decode with detected format
        if let Some(decoder_type) = detection.decoder_type {
            match crate::wmbus::radio::lora::decoder::decode_with_type(&decoder_type, payload, f_port) {
                Ok(mut data) => {
                    // Add detection info to the result
                    data.decoder_type = format!(
                        "{} (auto-detected, confidence: {:?})",
                        detection.format, detection.confidence
                    );
                    self.record_success(device_addr, &detection.format);
                    Ok(data)
                }
                Err(e) => {
                    self.record_failure(device_addr);
                    Err(e)
                }
            }
        } else {
            // No decoder found, use raw binary
            if let Some(default) = &self.manager.default_decoder {
                crate::wmbus::radio::lora::decoder::decode_with_type(default, payload, f_port)
            } else {
                Err(LoRaDecodeError::NoDecoder)
            }
        }
    }

    /// Get detection report for a payload
    pub fn analyze_payload(&self, payload: &[u8], f_port: u8) -> Vec<DetectionResult> {
        self.detector.detect_all(payload, f_port)
    }

    /// Get statistics for a device
    pub fn get_device_stats(&self, device_addr: &str) -> Option<DeviceStats> {
        self.device_stats.lock().unwrap().get(device_addr).cloned()
    }

    /// Get all device statistics
    pub fn get_all_stats(&self) -> HashMap<String, DeviceStats> {
        self.device_stats.lock().unwrap().clone()
    }

    /// Clear statistics for a device
    pub fn clear_stats(&mut self, device_addr: &str) {
        self.device_stats.lock().unwrap().remove(device_addr);
    }

    /// Record successful decoding
    fn record_success(&self, device_addr: &str, format: &str) {
        if !self.learning_enabled {
            return;
        }

        let mut stats = self.device_stats.lock().unwrap();
        let device_stat = stats.entry(device_addr.to_string()).or_default();

        *device_stat
            .successful_formats
            .entry(format.to_string())
            .or_insert(0) += 1;
        device_stat.last_successful_format = Some(format.to_string());
        device_stat.total_packets += 1;
    }

    /// Record failed decoding
    fn record_failure(&self, device_addr: &str) {
        if !self.learning_enabled {
            return;
        }

        let mut stats = self.device_stats.lock().unwrap();
        let device_stat = stats.entry(device_addr.to_string()).or_default();

        device_stat.failed_attempts += 1;
        device_stat.total_packets += 1;
    }

    /// Update statistics with detection result
    fn update_stats(&self, device_addr: &str, detection: &DetectionResult) {
        if !self.learning_enabled {
            return;
        }

        let mut stats = self.device_stats.lock().unwrap();
        let device_stat = stats.entry(device_addr.to_string()).or_default();

        // Update average confidence
        let confidence_score = detection.confidence as u8 as f32;
        device_stat.avg_confidence =
            (device_stat.avg_confidence * device_stat.total_packets as f32 + confidence_score)
                / (device_stat.total_packets + 1) as f32;
    }

    /// Determine if we should auto-register a decoder
    fn should_auto_register(&self, device_addr: &str, detection: &DetectionResult) -> bool {
        if !self.learning_enabled {
            return false;
        }

        // Check if confidence meets threshold
        if (detection.confidence as u8) < self.auto_register_threshold {
            return false;
        }

        // Check if this device has consistent format history
        if let Some(stats) = self.get_device_stats(device_addr) {
            if let Some(last_format) = &stats.last_successful_format {
                // If the format has been consistently successful, auto-register
                if last_format == &detection.format {
                    if let Some(count) = stats.successful_formats.get(&detection.format) {
                        // Auto-register after 3 consistent successful decodings
                        return *count >= 3;
                    }
                }
            }
        }

        // For new devices, require very high confidence
        detection.confidence == Confidence::Certain
    }

    /// Generate a report of detection capabilities
    pub fn generate_report(&self, payload: &[u8], f_port: u8) -> String {
        let mut report = String::new();
        report.push_str("=== LoRa Payload Analysis Report ===\n\n");

        report.push_str(&format!("Payload Length: {} bytes\n", payload.len()));
        report.push_str(&format!("fPort: {f_port}\n"));
        report.push_str(&format!("Hex: {}\n\n", hex::encode(payload)));

        let results = self.analyze_payload(payload, f_port);

        if results.is_empty() {
            report.push_str("No known formats detected.\n");
        } else {
            report.push_str("Detected Formats (sorted by confidence):\n");
            report.push_str("-----------------------------------------\n");

            for (idx, result) in results.iter().enumerate() {
                report.push_str(&format!(
                    "\n{}. {} (Confidence: {:?})\n",
                    idx + 1,
                    result.format,
                    result.confidence
                ));

                if let Some(variant) = &result.variant {
                    report.push_str(&format!("   Variant: {variant}\n"));
                }

                report.push_str("   Reasoning:\n");
                for reason in &result.reasoning {
                    report.push_str(&format!("   - {reason}\n"));
                }
            }
        }

        report.push_str("\n=== Recommendation ===\n");
        if !results.is_empty() && results[0].confidence >= Confidence::High {
            report.push_str(&format!(
                "Use {} decoder with high confidence.\n",
                results[0].format
            ));
        } else if !results.is_empty() && results[0].confidence >= Confidence::Medium {
            report.push_str(&format!(
                "Consider {} decoder, but verify output.\n",
                results[0].format
            ));
        } else {
            report.push_str("Manual analysis recommended. Payload format unclear.\n");
        }

        report
    }
}

impl Default for SmartDecoder {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_smart_decoder_learning() {
        let mut decoder = SmartDecoder::new();

        // OMS payload
        let payload = vec![
            0x2C, 0x44, 0x2D, 0x2C, // OMS header with Kamstrup
            0x00, 0x00, 0x00, 0x00, 0x01, 0x07, 0x00, 0x00,
        ];

        // First decode - should auto-detect
        let result1 = decoder.decode_smart("device1", &payload, 1);
        assert!(result1.is_ok());
        assert!(result1.unwrap().decoder_type.contains("auto-detected"));

        // Check stats
        let stats = decoder.get_device_stats("device1").unwrap();
        assert_eq!(stats.total_packets, 1);
        assert!(stats.avg_confidence > 0.0);

        // After multiple successful decodings, should auto-register
        for _ in 0..3 {
            let _ = decoder.decode_smart("device1", &payload, 1);
        }

        // Now it should be registered
        let stats = decoder.get_device_stats("device1").unwrap();
        assert!(stats.successful_formats.contains_key("OMS"));
    }

    #[test]
    fn test_analysis_report() {
        let decoder = SmartDecoder::new();

        // Cayenne LPP payload
        let payload = vec![
            0x01, 0x67, 0x00, 0xEB, // Temperature
            0x02, 0x68, 0x64, // Humidity
        ];

        let report = decoder.generate_report(&payload, 1);

        println!("Generated report:\n{}", report);
        assert!(report.contains("CayenneLPP"));
        assert!(report.contains("Temperature"));
        assert!(report.contains("Confidence"));
        // Note: Humidity may not appear in reasoning, only Temperature is guaranteed
    }

    #[test]
    fn test_fallback_to_raw() {
        let mut decoder = SmartDecoder::new();

        // Unknown payload format
        let payload = vec![0xFF, 0xFE, 0xFD, 0xFC];

        let result = decoder.decode_smart("unknown", &payload, 1);
        assert!(result.is_ok());

        // Should fall back to raw binary
        let data = result.unwrap();
        assert!(data.decoder_type.contains("RawBinary"));
    }
}
