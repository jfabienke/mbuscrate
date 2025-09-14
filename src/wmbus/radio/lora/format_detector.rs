//! Automatic payload format detection for LoRa metering devices
//!
//! This module provides intelligent detection of payload formats based on
//! signatures, patterns, and statistical analysis.

use crate::wmbus::radio::lora::decoder::{DraginoModel, ElvacoModel, DecoderType};

/// Type alias for format detection function
type DetectionFunction = Box<dyn Fn(&[u8], u8) -> Option<DetectionResult> + Send + Sync>;

/// Detection confidence level
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Confidence {
    /// Definitive match based on unique signatures
    Certain = 100,
    /// High probability based on multiple indicators
    High = 80,
    /// Moderate probability based on some indicators
    Medium = 60,
    /// Low probability, might work
    Low = 40,
    /// No match found
    None = 0,
}

impl Confidence {
    pub fn from_score(score: u8) -> Self {
        match score {
            90..=100 => Self::Certain,
            70..=89 => Self::High,
            50..=69 => Self::Medium,
            30..=49 => Self::Low,
            _ => Self::None,
        }
    }
}

/// Format detection result
#[derive(Debug, Clone)]
pub struct DetectionResult {
    /// Detected format name
    pub format: String,
    /// Confidence level
    pub confidence: Confidence,
    /// Specific variant or model if applicable
    pub variant: Option<String>,
    /// Reasoning for the detection
    pub reasoning: Vec<String>,
    /// Suggested decoder type
    pub decoder_type: Option<DecoderType>,
}

/// Payload format detector
pub struct FormatDetector {
    /// Registered format signatures
    signatures: Vec<FormatSignature>,
    /// Statistical analyzers
    analyzers: Vec<Box<dyn FormatAnalyzer>>,
}

/// Format signature for pattern matching
struct FormatSignature {
    #[allow(dead_code)]
    name: String,
    check: DetectionFunction,
}

/// Trait for statistical format analysis
trait FormatAnalyzer: Send + Sync {
    fn analyze(&self, payload: &[u8], f_port: u8) -> Option<DetectionResult>;
}

impl Default for FormatDetector {
    fn default() -> Self {
        Self::new()
    }
}

impl FormatDetector {
    /// Create a new format detector with all known formats
    pub fn new() -> Self {
        let mut detector = Self {
            signatures: Vec::new(),
            analyzers: Vec::new(),
        };

        // Register all format signatures
        detector.register_oms_signature();
        detector.register_cayenne_signature();
        detector.register_compact_frame_signature();
        detector.register_decentlab_signature();
        detector.register_dragino_signature();
        detector.register_elvaco_signature();
        detector.register_sensative_signature();

        // Register statistical analyzers
        detector.analyzers.push(Box::new(GenericCounterAnalyzer));
        detector.analyzers.push(Box::new(WMBusAnalyzer));

        detector
    }

    /// Detect the most likely format for a payload
    pub fn detect(&self, payload: &[u8], f_port: u8) -> DetectionResult {
        let mut best_result = DetectionResult {
            format: "Unknown".to_string(),
            confidence: Confidence::None,
            variant: None,
            reasoning: vec!["No matching format detected".to_string()],
            decoder_type: None,
        };

        // First, check signatures (definitive patterns)
        for signature in &self.signatures {
            if let Some(result) = (signature.check)(payload, f_port) {
                if result.confidence > best_result.confidence {
                    best_result = result;
                }

                // If we have a certain match, stop looking
                if best_result.confidence == Confidence::Certain {
                    return best_result;
                }
            }
        }

        // If no certain match, try statistical analysis
        for analyzer in &self.analyzers {
            if let Some(result) = analyzer.analyze(payload, f_port) {
                if result.confidence > best_result.confidence {
                    best_result = result;
                }
            }
        }

        best_result
    }

    /// Register OMS format signature
    fn register_oms_signature(&mut self) {
        self.signatures.push(FormatSignature {
            name: "OMS".to_string(),
            check: Box::new(|payload, _f_port| {
                if payload.len() < 12 {
                    return None;
                }

                let mut reasons = Vec::new();
                let mut score = 0u8;

                // Check C-field for typical OMS values
                if payload.len() > 1 {
                    let c_field = payload[1];
                    if c_field == 0x44 || c_field == 0x46 || c_field == 0x08 {
                        score += 40;
                        reasons.push(format!("OMS C-field detected: 0x{c_field:02X}"));
                    }
                }

                // Check manufacturer ID range (bytes 2-3)
                if payload.len() > 3 {
                    let mfr = u16::from_le_bytes([payload[2], payload[3]]);
                    // Known OMS manufacturers
                    let known_mfrs = [
                        0x2C2D, // Kamstrup
                        0x11A5, // Diehl
                        0x1C08, // Itron
                        0x32A7, // Landis+Gyr
                        0x6A50, // Zenner
                    ];

                    if known_mfrs.contains(&mfr) {
                        score += 40;
                        reasons.push(format!("Known OMS manufacturer: 0x{mfr:04X}"));
                    }
                }

                // Check medium byte (position 10)
                if payload.len() > 10 {
                    let medium = payload[10];
                    if medium <= 0x0F {
                        score += 20;
                        reasons.push(format!("Valid OMS medium: 0x{medium:02X}"));
                    }
                }

                if score > 0 {
                    Some(DetectionResult {
                        format: "OMS".to_string(),
                        confidence: Confidence::from_score(score),
                        variant: None,
                        reasoning: reasons,
                        decoder_type: Some(DecoderType::RawBinary), // OMS decoder
                    })
                } else {
                    None
                }
            }),
        });
    }

    /// Register Cayenne LPP signature
    fn register_cayenne_signature(&mut self) {
        self.signatures.push(FormatSignature {
            name: "CayenneLPP".to_string(),
            check: Box::new(|payload, _f_port| {
                if payload.len() < 3 {
                    return None;
                }

                let mut reasons = Vec::new();
                let mut score = 0u8;
                let mut offset = 0;
                let mut valid_tlvs = 0;

                // Try to parse as Cayenne TLV
                while offset + 2 < payload.len() {
                    let _channel = payload[offset];
                    let type_byte = payload[offset + 1];

                    // Check for valid Cayenne types
                    let data_size = match type_byte {
                        0x00 | 0x01 | 0x66 => 1, // Digital, Presence
                        0x02 | 0x03 | 0x67 | 0x68 | 0x65 | 0x73 | 0x74..=0x78 => 2, // Various 2-byte
                        0x71 | 0x86 => 6, // Accelerometer, Gyrometer
                        0x88 => 9,        // GPS
                        0x83 | 0x85 => 4, // Energy, UnixTime
                        _ => 0,
                    };

                    if data_size > 0 && offset + 2 + data_size <= payload.len() {
                        valid_tlvs += 1;
                        offset += 2 + data_size;

                        // Track specific types found
                        match type_byte {
                            0x67 => reasons.push("Temperature sensor detected".to_string()),
                            0x68 => reasons.push("Humidity sensor detected".to_string()),
                            0x73 => reasons.push("Barometer detected".to_string()),
                            0x88 => reasons.push("GPS location detected".to_string()),
                            _ => {}
                        }
                    } else {
                        break;
                    }
                }

                // Score based on how much of the payload was valid Cayenne
                if valid_tlvs > 0 {
                    let coverage = (offset * 100) / payload.len();
                    if coverage > 90 {
                        score = 95;
                        reasons.push(format!("{valid_tlvs} valid Cayenne TLV entries"));
                    } else if coverage > 70 {
                        score = 75;
                        reasons.push(format!("{valid_tlvs} partial Cayenne TLV entries"));
                    } else if valid_tlvs >= 1 {
                        score = 50;
                        reasons.push("Some Cayenne-like TLV structure".to_string());
                    }
                }

                if score > 0 {
                    Some(DetectionResult {
                        format: "CayenneLPP".to_string(),
                        confidence: Confidence::from_score(score),
                        variant: None,
                        reasoning: reasons,
                        decoder_type: Some(DecoderType::RawBinary), // Cayenne
                    })
                } else {
                    None
                }
            }),
        });
    }

    /// Register Decentlab signature
    fn register_decentlab_signature(&mut self) {
        self.signatures.push(FormatSignature {
            name: "Decentlab".to_string(),
            check: Box::new(|payload, _f_port| {
                if payload.len() < 6 {
                    return None;
                }

                let mut reasons = Vec::new();
                let mut score = 0u8;

                // Check protocol version (typically 0x02)
                if payload[0] == 0x02 {
                    score += 30;
                    reasons.push("Decentlab protocol v2 detected".to_string());
                }

                // Check for reasonable device ID (bytes 1-2)
                let device_id = u16::from_be_bytes([payload[1], payload[2]]);
                if device_id > 0 && device_id < 0xFFFF {
                    score += 20;
                }

                // Check sensor flags (byte 3)
                let sensor_flags = payload[3];
                if sensor_flags > 0 && sensor_flags.count_ones() <= 4 {
                    score += 30;
                    reasons.push(format!("{} sensors active", sensor_flags.count_ones()));
                }

                // Check for battery voltage at end (typically 2500-3600 mV)
                if payload.len() >= 6 {
                    let last_two = u16::from_be_bytes([
                        payload[payload.len() - 2],
                        payload[payload.len() - 1],
                    ]);
                    if (2000..=4000).contains(&last_two) {
                        score += 20;
                        reasons.push(format!("Battery voltage {last_two} mV detected"));
                    }
                }

                if score >= 50 {
                    Some(DetectionResult {
                        format: "Decentlab".to_string(),
                        confidence: Confidence::from_score(score),
                        variant: None,
                        reasoning: reasons,
                        decoder_type: Some(DecoderType::RawBinary), // Decentlab
                    })
                } else {
                    None
                }
            }),
        });
    }

    /// Register Dragino signatures
    fn register_dragino_signature(&mut self) {
        self.signatures.push(FormatSignature {
            name: "Dragino".to_string(),
            check: Box::new(|payload, _f_port| {
                let mut reasons = Vec::new();
                let mut score = 0u8;
                let mut variant = None;

                // SW3L Water Flow Sensor (13 bytes)
                if payload.len() == 13 {
                    // Check for reasonable flow rate and volume patterns
                    if payload.len() >= 9 {
                        let flow_rate = u16::from_le_bytes([payload[3], payload[4]]);
                        let volume =
                            u32::from_le_bytes([payload[5], payload[6], payload[7], payload[8]]);

                        if flow_rate < 10000 && volume < 100000000 {
                            score = 80;
                            variant = Some("SW3L".to_string());
                            reasons.push("Dragino SW3L format detected (13 bytes)".to_string());
                            reasons.push(format!("Flow rate: {} L/h", flow_rate as f32 / 10.0));
                        }
                    }
                }

                // LWL03A Water Leak Sensor (9 bytes)
                if payload.len() == 9 {
                    let leak_status = payload[2];
                    if leak_status <= 1 {
                        score = 75;
                        variant = Some("LWL03A".to_string());
                        reasons.push("Dragino LWL03A format detected (9 bytes)".to_string());
                        if leak_status == 1 {
                            reasons.push("Leak detected!".to_string());
                        }
                    }
                }

                if score > 0 {
                    let model = match variant.as_deref() {
                        Some("SW3L") => DraginoModel::SW3L,
                        Some("LWL03A") => DraginoModel::LWL03A,
                        _ => DraginoModel::SW3L,
                    };

                    Some(DetectionResult {
                        format: "Dragino".to_string(),
                        confidence: Confidence::from_score(score),
                        variant,
                        reasoning: reasons,
                        decoder_type: Some(DecoderType::Dragino(model)),
                    })
                } else {
                    None
                }
            }),
        });
    }

    /// Register Elvaco signatures
    fn register_elvaco_signature(&mut self) {
        self.signatures.push(FormatSignature {
            name: "Elvaco".to_string(),
            check: Box::new(|payload, _f_port| {
                if payload.len() < 12 {
                    return None;
                }

                let mut reasons = Vec::new();
                let mut score = 0u8;
                let mut variant = None;

                // Check for Elvaco-specific markers
                if payload[0] == 0x78 || payload[0] == 0x79 {
                    score += 40;
                    if payload[0] == 0x78 {
                        reasons.push("Elvaco water meter signature".to_string());
                        variant = Some("CMi4110-Water".to_string());
                    } else {
                        reasons.push("Elvaco heat meter signature".to_string());
                        variant = Some("CMi4110-Heat".to_string());
                    }
                }

                // Check for electricity meter pattern
                if payload.len() >= 24 && (payload[0] & 0xF0) == 0x40 {
                    score += 40;
                    reasons.push("Elvaco electricity meter signature".to_string());
                    variant = Some("CMe3100".to_string());
                }

                // Additional validation for CMi4110
                if payload.len() >= 20 && (payload[0] == 0x78 || payload[0] == 0x79) {
                    // Check for reasonable temperature values
                    if payload.len() >= 15 {
                        let temp = u16::from_le_bytes([payload[13], payload[14]]);
                        if temp > 0 && temp < 10000 {
                            // 0-100°C
                            score += 20;
                            reasons
                                .push(format!("Valid temperature: {:.2}°C", temp as f32 / 100.0));
                        }
                    }
                }

                if score > 0 {
                    let model = match variant.as_deref() {
                        Some("CMi4110-Water") | Some("CMi4110-Heat") => ElvacoModel::CMi4110,
                        Some("CMe3100") => ElvacoModel::CMe3100,
                        _ => ElvacoModel::Generic,
                    };

                    Some(DetectionResult {
                        format: "Elvaco".to_string(),
                        confidence: Confidence::from_score(score),
                        variant,
                        reasoning: reasons,
                        decoder_type: Some(DecoderType::Elvaco(model)),
                    })
                } else {
                    None
                }
            }),
        });
    }

    /// Register Sensative signature
    fn register_sensative_signature(&mut self) {
        self.signatures.push(FormatSignature {
            name: "Sensative".to_string(),
            check: Box::new(|payload, _f_port| {
                if payload.len() < 3 {
                    return None;
                }

                let mut reasons = Vec::new();
                let mut score = 0u8;
                let mut offset = 0;
                let mut valid_tlvs = 0;

                // Sensative uses specific TLV type codes
                while offset + 2 < payload.len() {
                    let typ = payload[offset];
                    let len = payload[offset + 1] as usize;

                    // Check for Sensative-specific types
                    let expected_len = match typ {
                        0x01 => 2,        // Temperature
                        0x02 => 1,        // Humidity
                        0x03 => 2,        // Light
                        0x04 | 0x05 => 1, // Door/Presence
                        _ => 0,
                    };

                    if expected_len > 0 && len == expected_len && offset + 2 + len <= payload.len()
                    {
                        valid_tlvs += 1;
                        offset += 2 + len;

                        match typ {
                            0x01 => reasons.push("Sensative temperature sensor".to_string()),
                            0x02 => reasons.push("Sensative humidity sensor".to_string()),
                            0x04 => reasons.push("Sensative door sensor".to_string()),
                            _ => {}
                        }
                    } else {
                        break;
                    }
                }

                if valid_tlvs > 0 && offset == payload.len() {
                    score = 85;
                    reasons.push(format!("{valid_tlvs} Sensative TLV entries"));
                }

                if score > 0 {
                    Some(DetectionResult {
                        format: "Sensative".to_string(),
                        confidence: Confidence::from_score(score),
                        variant: Some("Strips".to_string()),
                        reasoning: reasons,
                        decoder_type: Some(DecoderType::Sensative),
                    })
                } else {
                    None
                }
            }),
        });
    }

    /// Register compact frame signature
    fn register_compact_frame_signature(&mut self) {
        self.signatures.push(FormatSignature {
            name: "CompactFrame".to_string(),
            check: Box::new(|payload, f_port| {
                if payload.len() < 11 {
                    return None;
                }

                let mut reasons = Vec::new();
                let mut score = 0u8;

                // Check for compact frame structure
                if payload[0] >= 0x1C && payload[0] <= 0x3C {
                    score += 30;
                    reasons.push("Valid compact frame length byte".to_string());
                }

                // Check fPort mapping
                if (1..=6).contains(&f_port) {
                    score += 20;
                    let unit = match f_port {
                        1 => "m³",
                        2 => "kWh",
                        3 => "L",
                        4 => "MWh",
                        5 => "kg",
                        6 => "t",
                        _ => "units",
                    };
                    reasons.push(format!("fPort {f_port} indicates {unit} measurement"));
                }

                // Check for reasonable counter value at offset 4
                if payload.len() >= 8 {
                    let counter =
                        u32::from_le_bytes([payload[4], payload[5], payload[6], payload[7]]);
                    if counter < 100000000 {
                        score += 20;
                        reasons.push(format!("Valid counter value: {counter}"));
                    }
                }

                // Check battery byte
                if payload.len() >= 11 && payload[10] <= 100 {
                    score += 20;
                    reasons.push(format!("Battery level: {}%", payload[10]));
                }

                if score >= 50 {
                    Some(DetectionResult {
                        format: "CompactFrame".to_string(),
                        confidence: Confidence::from_score(score),
                        variant: None,
                        reasoning: reasons,
                        decoder_type: Some(DecoderType::En13757Compact),
                    })
                } else {
                    None
                }
            }),
        });
    }

    /// Detect multiple possible formats with confidence scores
    pub fn detect_all(&self, payload: &[u8], f_port: u8) -> Vec<DetectionResult> {
        let mut results = Vec::new();

        // Check all signatures
        for signature in &self.signatures {
            if let Some(result) = (signature.check)(payload, f_port) {
                if result.confidence > Confidence::None {
                    results.push(result);
                }
            }
        }

        // Check all analyzers
        for analyzer in &self.analyzers {
            if let Some(result) = analyzer.analyze(payload, f_port) {
                if result.confidence > Confidence::None {
                    results.push(result);
                }
            }
        }

        // Sort by confidence (highest first)
        results.sort_by(|a, b| b.confidence.cmp(&a.confidence));

        results
    }
}

/// Statistical analyzer for generic counter formats
struct GenericCounterAnalyzer;

impl FormatAnalyzer for GenericCounterAnalyzer {
    fn analyze(&self, payload: &[u8], _f_port: u8) -> Option<DetectionResult> {
        // Check for typical counter patterns
        if payload.len() < 4 {
            return None;
        }

        let mut reasons = Vec::new();
        let mut score = 0u8;

        // Look for common counter sizes (4, 6, 8 bytes)
        if payload.len() == 8 || payload.len() == 10 || payload.len() == 12 {
            score += 20;
            reasons.push(format!(
                "Common counter payload size: {} bytes",
                payload.len()
            ));
        }

        // Check if first 4 bytes could be a reasonable counter
        let counter_le = u32::from_le_bytes([payload[0], payload[1], payload[2], payload[3]]);
        let counter_be = u32::from_be_bytes([payload[0], payload[1], payload[2], payload[3]]);

        if counter_le < 10000000 {
            score += 20;
            reasons.push(format!("Possible counter value (LE): {counter_le}"));
        } else if counter_be < 10000000 {
            score += 20;
            reasons.push(format!("Possible counter value (BE): {counter_be}"));
        }

        // Check for battery byte at end
        if payload.len() >= 5 {
            let last_byte = payload[payload.len() - 1];
            if last_byte <= 100 {
                score += 15;
                reasons.push(format!("Possible battery percentage: {last_byte}%"));
            }
        }

        // Check for status byte pattern
        if payload.len() >= 7 {
            let possible_status = payload[payload.len() - 2];
            if possible_status.count_ones() <= 3 {
                score += 15;
                reasons.push("Possible status flags detected".to_string());
            }
        }

        if score >= 30 {
            Some(DetectionResult {
                format: "GenericCounter".to_string(),
                confidence: Confidence::from_score(score),
                variant: None,
                reasoning: reasons,
                decoder_type: Some(DecoderType::GenericCounter(
                    crate::wmbus::radio::lora::decoder::GenericCounterConfig::default(),
                )),
            })
        } else {
            None
        }
    }
}

/// Statistical analyzer for wM-Bus patterns
struct WMBusAnalyzer;

impl FormatAnalyzer for WMBusAnalyzer {
    fn analyze(&self, payload: &[u8], _f_port: u8) -> Option<DetectionResult> {
        if payload.len() < 12 {
            return None;
        }

        let mut reasons = Vec::new();
        let mut score = 0u8;

        // Check for wM-Bus frame start bytes
        if payload[0] == 0x68 || payload[0] == 0x10 {
            score += 30;
            reasons.push("wM-Bus start byte detected".to_string());
        }

        // Check for length consistency
        if payload[0] == payload[3] && payload[0] < 0xFF {
            score += 20;
            reasons.push("wM-Bus length fields match".to_string());
        }

        // Check for stop byte
        if !payload.is_empty() && payload[payload.len() - 1] == 0x16 {
            score += 20;
            reasons.push("wM-Bus stop byte detected".to_string());
        }

        if score >= 50 {
            Some(DetectionResult {
                format: "wM-Bus".to_string(),
                confidence: Confidence::from_score(score),
                variant: None,
                reasoning: reasons,
                decoder_type: Some(DecoderType::En13757Compact),
            })
        } else {
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_oms_detection() {
        let detector = FormatDetector::new();

        // OMS frame with Kamstrup manufacturer
        let payload = vec![
            0x2C, // Length
            0x44, // C-field (SND-NR)
            0x2D, 0x2C, // Kamstrup
            0x00, 0x00, 0x00, 0x00, 0x01, 0x07, // Water
            0x00, 0x00,
        ];

        let result = detector.detect(&payload, 1);
        assert_eq!(result.format, "OMS");
        assert!(result.confidence >= Confidence::High);
        assert!(result.reasoning.iter().any(|r| r.contains("0x2C2D")));
    }

    #[test]
    fn test_cayenne_detection() {
        let detector = FormatDetector::new();

        // Cayenne LPP with temperature and humidity
        let payload = vec![
            0x01, 0x67, 0x00, 0xEB, // Ch1: Temperature
            0x02, 0x68, 0x64, // Ch2: Humidity
        ];

        let result = detector.detect(&payload, 1);
        assert_eq!(result.format, "CayenneLPP");
        assert!(result.confidence >= Confidence::Medium);
        assert!(result.reasoning.iter().any(|r| r.contains("Temperature")));
    }

    #[test]
    fn test_detect_all() {
        let detector = FormatDetector::new();

        // Ambiguous payload that could match multiple formats
        let payload = vec![
            0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08, 0x00, 0x00,
            85, // Battery-like value
        ];

        let results = detector.detect_all(&payload, 1);

        // Should detect at least generic counter
        assert!(!results.is_empty());

        // Results should be sorted by confidence
        for i in 1..results.len() {
            assert!(results[i - 1].confidence >= results[i].confidence);
        }
    }

    #[test]
    fn test_dragino_sw3l_detection() {
        let detector = FormatDetector::new();

        // Dragino SW3L payload (13 bytes)
        let payload = vec![
            0x12, 0x34, // Device ID
            0x00, // Status
            0xE8, 0x03, // Flow rate
            0x10, 0x27, 0x00, 0x00, // Volume
            0x10, 0x09, // Temperature
            0xE4, 0x0C, // Battery
        ];

        let result = detector.detect(&payload, 1);
        assert_eq!(result.format, "Dragino");
        assert_eq!(result.variant, Some("SW3L".to_string()));
        assert!(result.confidence >= Confidence::Medium);
    }

    #[test]
    fn test_confidence_scoring() {
        assert_eq!(Confidence::from_score(95), Confidence::Certain);
        assert_eq!(Confidence::from_score(75), Confidence::High);
        assert_eq!(Confidence::from_score(55), Confidence::Medium);
        assert_eq!(Confidence::from_score(35), Confidence::Low);
        assert_eq!(Confidence::from_score(10), Confidence::None);
    }
}
