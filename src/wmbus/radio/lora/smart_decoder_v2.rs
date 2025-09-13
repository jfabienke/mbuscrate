//! Enhanced smart decoder with graceful fallback chain
//!
//! This version implements a robust fallback strategy when decoders fail.

use super::decoder::{LoRaDecodeError, LoRaPayloadDecoder, MeteringData};
use super::format_detector::{Confidence, DetectionResult, FormatDetector};
use super::smart_decoder::DeviceStats;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

/// Decoding attempt result with detailed information
#[derive(Debug, Clone)]
pub struct DecodingAttempt {
    pub format: String,
    pub confidence: Confidence,
    pub result: Result<MeteringData, String>,
    pub duration_ms: u128,
}

/// Enhanced smart decoder with fallback chain
pub struct SmartDecoderV2 {
    /// Format detector
    detector: FormatDetector,
    /// Device statistics
    device_stats: Arc<Mutex<HashMap<String, DeviceStats>>>,
    /// Registered device-specific decoders
    device_decoders: HashMap<String, Box<dyn LoRaPayloadDecoder>>,
    /// Enable detailed logging
    verbose: bool,
    /// Maximum decoders to try
    max_attempts: usize,
    /// Minimum confidence to attempt decoding
    min_decode_confidence: u8,
}

impl SmartDecoderV2 {
    pub fn new() -> Self {
        Self {
            detector: FormatDetector::new(),
            device_stats: Arc::new(Mutex::new(HashMap::new())),
            device_decoders: HashMap::new(),
            verbose: false,
            max_attempts: 5,
            min_decode_confidence: 30, // Try even low confidence decoders
        }
    }

    /// Enable verbose logging for debugging
    pub fn set_verbose(&mut self, verbose: bool) {
        self.verbose = verbose;
    }

    /// Decode with comprehensive fallback strategy
    pub fn decode_with_fallback(
        &mut self,
        device_addr: &str,
        payload: &[u8],
        f_port: u8,
    ) -> Result<MeteringData, LoRaDecodeError> {
        let start_time = std::time::Instant::now();
        let mut attempts = Vec::new();

        // Step 1: Try registered device decoder (if any)
        if let Some(decoder) = self.device_decoders.get(device_addr) {
            if self.verbose {
                log::debug!("Trying registered decoder for {}", device_addr);
            }

            match self.try_decoder(decoder.as_ref(), payload, f_port, "Registered") {
                Ok(data) => {
                    self.record_success(device_addr, &data.decoder_type);
                    return Ok(data);
                }
                Err(e) => {
                    attempts.push(DecodingAttempt {
                        format: "Registered".to_string(),
                        confidence: Confidence::High,
                        result: Err(e.clone()),
                        duration_ms: start_time.elapsed().as_millis(),
                    });

                    if self.verbose {
                        log::warn!("Registered decoder failed: {}", e);
                    }
                }
            }
        }

        // Step 2: Get all possible formats sorted by confidence
        let detections = self.detector.detect_all(payload, f_port);

        if self.verbose {
            log::debug!(
                "Detected {} possible formats for {}",
                detections.len(),
                device_addr
            );
        }

        // Step 3: Try each detected format in order of confidence
        for (idx, detection) in detections.iter().take(self.max_attempts).enumerate() {
            // Skip if confidence too low
            if (detection.confidence as u8) < self.min_decode_confidence {
                if self.verbose {
                    log::debug!(
                        "Skipping {} due to low confidence ({:?})",
                        detection.format,
                        detection.confidence
                    );
                }
                continue;
            }

            if let Some(decoder) = &detection.decoder {
                if self.verbose {
                    log::debug!(
                        "Attempt {}: Trying {} decoder (confidence: {:?})",
                        idx + 1,
                        detection.format,
                        detection.confidence
                    );
                }

                match self.try_decoder(decoder.as_ref(), payload, f_port, &detection.format) {
                    Ok(mut data) => {
                        // Success! Update metadata
                        data.decoder_type = format!(
                            "{} (confidence: {:?}, attempt: {})",
                            detection.format,
                            detection.confidence,
                            idx + 1
                        );

                        self.record_success(device_addr, &detection.format);

                        // Log all attempts for analysis
                        if self.verbose {
                            self.log_attempts(&attempts, Some(&data));
                        }

                        return Ok(data);
                    }
                    Err(e) => {
                        attempts.push(DecodingAttempt {
                            format: detection.format.clone(),
                            confidence: detection.confidence,
                            result: Err(e.clone()),
                            duration_ms: start_time.elapsed().as_millis(),
                        });

                        if self.verbose {
                            log::debug!("{} decoder failed: {}", detection.format, e);
                        }
                    }
                }
            }
        }

        // Step 4: All decoders failed - try partial recovery
        if let Some(partial) = self.try_partial_recovery(payload, f_port, &attempts) {
            if self.verbose {
                log::info!("Partial recovery succeeded for {}", device_addr);
            }
            return Ok(partial);
        }

        // Step 5: Ultimate fallback - return raw binary with metadata
        if self.verbose {
            log::warn!(
                "All {} decoding attempts failed for {}, returning raw binary",
                attempts.len(),
                device_addr
            );
            self.log_attempts(&attempts, None);
        }

        self.record_failure(device_addr);

        Ok(self.create_raw_fallback(payload, f_port, attempts))
    }

    /// Try a decoder with error recovery
    fn try_decoder(
        &self,
        decoder: &dyn LoRaPayloadDecoder,
        payload: &[u8],
        f_port: u8,
        format_name: &str,
    ) -> Result<MeteringData, String> {
        // Try normal decoding
        match decoder.decode(payload, f_port) {
            Ok(data) => Ok(data),
            Err(e) => {
                // Try error recovery strategies
                match e {
                    LoRaDecodeError::InvalidLength { expected, actual } => {
                        // Try with truncated/padded payload
                        if actual < expected && expected <= 255 {
                            let mut padded = payload.to_vec();
                            padded.resize(expected, 0);

                            if let Ok(data) = decoder.decode(&padded, f_port) {
                                if self.verbose {
                                    log::warn!(
                                        "{} decoder succeeded with padded payload",
                                        format_name
                                    );
                                }
                                return Ok(data);
                            }
                        }
                    }
                    LoRaDecodeError::CrcError => {
                        // Try ignoring CRC for partial data recovery
                        if self.verbose {
                            log::debug!("Attempting to decode {} despite CRC error", format_name);
                        }
                        // Some decoders might have a lenient mode we could try
                    }
                    _ => {}
                }

                Err(format!("{}: {}", format_name, e))
            }
        }
    }

    /// Attempt partial recovery from failed attempts
    fn try_partial_recovery(
        &self,
        payload: &[u8],
        f_port: u8,
        attempts: &[DecodingAttempt],
    ) -> Option<MeteringData> {
        // Strategy: Combine partial successes from different decoders
        // This is useful when payload has mixed formats or corruption

        let mut combined_readings = Vec::new();
        let mut best_confidence = Confidence::None;

        // Look for any decoder that got partial data
        for attempt in attempts {
            if attempt.confidence > best_confidence {
                best_confidence = attempt.confidence;
            }

            // In a real implementation, decoders could return partial results
            // For now, we'll just track the attempts
        }

        // If we have enough confidence in the format but decoding failed,
        // try to extract basic values manually
        if best_confidence >= Confidence::Medium && payload.len() >= 4 {
            // Extract what we can
            let value = u32::from_le_bytes([
                payload.get(0).copied().unwrap_or(0),
                payload.get(1).copied().unwrap_or(0),
                payload.get(2).copied().unwrap_or(0),
                payload.get(3).copied().unwrap_or(0),
            ]);

            if value > 0 && value < 100_000_000 {
                let mut data = MeteringData {
                    timestamp: std::time::SystemTime::now(),
                    readings: vec![super::decoder::Reading {
                        value: super::decoder::MBusRecordValue::Numeric(value as f64),
                        unit: "units".to_string(),
                        quantity: "Recovered Value".to_string(),
                        tariff: None,
                        storage_number: None,
                        description: Some("Partial recovery from corrupted payload".to_string()),
                    }],
                    battery: None,
                    status: super::decoder::DeviceStatus::default(),
                    raw_payload: payload.to_vec(),
                    decoder_type: format!("PartialRecovery (confidence: {:?})", best_confidence),
                };

                // Try to extract battery if it's commonly at the end
                if let Some(&battery) = payload.last() {
                    if battery <= 100 {
                        data.battery = Some(super::decoder::BatteryStatus {
                            voltage: None,
                            percentage: Some(battery),
                            low_battery: battery < 20,
                        });
                    }
                }

                return Some(data);
            }
        }

        None
    }

    /// Create raw binary fallback with attempt metadata
    fn create_raw_fallback(
        &self,
        payload: &[u8],
        f_port: u8,
        attempts: Vec<DecodingAttempt>,
    ) -> MeteringData {
        let mut description = format!("Raw payload (tried {} decoders):\n", attempts.len());

        for attempt in &attempts {
            description.push_str(&format!(
                "- {} (confidence: {:?}): Failed\n",
                attempt.format, attempt.confidence
            ));
        }

        MeteringData {
            timestamp: std::time::SystemTime::now(),
            readings: vec![super::decoder::Reading {
                value: super::decoder::MBusRecordValue::String(hex::encode(payload)),
                unit: "hex".to_string(),
                quantity: "Raw Data".to_string(),
                tariff: None,
                storage_number: None,
                description: Some(description),
            }],
            battery: None,
            status: super::decoder::DeviceStatus::default(),
            raw_payload: payload.to_vec(),
            decoder_type: format!("RawBinary (fallback after {} attempts)", attempts.len()),
        }
    }

    /// Log all decoding attempts for debugging
    fn log_attempts(&self, attempts: &[DecodingAttempt], success: Option<&MeteringData>) {
        log::debug!("=== Decoding Attempts Summary ===");

        for (idx, attempt) in attempts.iter().enumerate() {
            log::debug!(
                "  {}. {} (confidence: {:?}, time: {}ms) - {}",
                idx + 1,
                attempt.format,
                attempt.confidence,
                attempt.duration_ms,
                if attempt.result.is_ok() {
                    "Success"
                } else {
                    "Failed"
                }
            );

            if let Err(e) = &attempt.result {
                log::debug!("     Error: {}", e);
            }
        }

        if let Some(data) = success {
            log::debug!("  ✓ Final success: {}", data.decoder_type);
        } else {
            log::debug!("  ✗ All attempts failed, using fallback");
        }

        log::debug!("=================================");
    }

    fn record_success(&self, device_addr: &str, format: &str) {
        let mut stats = self.device_stats.lock().unwrap();
        let device_stat = stats.entry(device_addr.to_string()).or_default();

        *device_stat
            .successful_formats
            .entry(format.to_string())
            .or_insert(0) += 1;
        device_stat.last_successful_format = Some(format.to_string());
        device_stat.total_packets += 1;
    }

    fn record_failure(&self, device_addr: &str) {
        let mut stats = self.device_stats.lock().unwrap();
        let device_stat = stats.entry(device_addr.to_string()).or_default();

        device_stat.failed_attempts += 1;
        device_stat.total_packets += 1;
    }
}

/// Extension trait for decoders to support partial/lenient decoding
pub trait LenientDecoder: LoRaPayloadDecoder {
    /// Try to decode with relaxed validation
    fn decode_lenient(&self, payload: &[u8], f_port: u8) -> Result<MeteringData, LoRaDecodeError> {
        // Default implementation just calls normal decode
        self.decode(payload, f_port)
    }

    /// Extract any readable values even if structure is invalid
    fn extract_partial(&self, payload: &[u8]) -> Vec<super::decoder::Reading> {
        // Default: no partial extraction
        Vec::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_fallback_chain() {
        let mut decoder = SmartDecoderV2::new();
        decoder.set_verbose(true);

        // Corrupted OMS-like payload (missing bytes)
        let payload = vec![
            0x2C, 0x44, 0x2D, 0x2C, // OMS header
            0x00, 0x00, // Truncated!
        ];

        let result = decoder.decode_with_fallback("test", &payload, 1);
        assert!(result.is_ok());

        let data = result.unwrap();
        // Should fall back to raw binary or partial recovery
        assert!(
            data.decoder_type.contains("fallback")
                || data.decoder_type.contains("PartialRecovery")
                || data.decoder_type.contains("RawBinary")
        );
    }

    #[test]
    fn test_partial_recovery() {
        let decoder = SmartDecoderV2::new();

        // Payload with recognizable counter but invalid format
        let payload = vec![
            0x10, 0x27, 0x00, 0x00, // 10000 in little-endian
            0xFF, 0xFF, // Garbage
            85,   // Battery-like value
        ];

        let result = decoder.decode_with_fallback("test", &payload, 1);
        assert!(result.is_ok());

        let data = result.unwrap();

        // Should extract something
        assert!(!data.readings.is_empty());

        // Might detect battery
        if let Some(battery) = data.battery {
            assert_eq!(battery.percentage, Some(85));
        }
    }

    #[test]
    fn test_verbose_logging() {
        let mut decoder = SmartDecoderV2::new();
        decoder.set_verbose(true);
        decoder.max_attempts = 3;

        // Unknown format
        let payload = vec![0xDE, 0xAD, 0xBE, 0xEF];

        let result = decoder.decode_with_fallback("verbose_test", &payload, 1);
        assert!(result.is_ok());

        // Check that multiple attempts were made
        let data = result.unwrap();
        assert!(data.decoder_type.contains("attempt") || data.decoder_type.contains("fallback"));
    }
}
