//! EN 13757-3 Compact Frame decoder for LoRa payloads
//!
//! This decoder handles wM-Bus compact frames (Type F) transmitted over LoRa.
//! It leverages the existing M-Bus parsing infrastructure.

use crate::payload::record::MBusRecordValue;
use crate::wmbus::frame::parse_wmbus_frame;
use crate::wmbus::radio::lora::decoder::{
    BatteryStatus, DeviceStatus, LoRaDecodeError, LoRaPayloadDecoder, MeteringData, Reading,
};
use std::time::SystemTime;

/// Decoder for EN 13757-3 Compact Frame format
#[derive(Debug, Clone)]
pub struct CompactFrameDecoder {
    /// Expected manufacturer ID (optional filter)
    pub manufacturer_id: Option<u16>,
    /// Enable extended parsing for manufacturer-specific data
    pub parse_extended: bool,
}

impl Default for CompactFrameDecoder {
    fn default() -> Self {
        Self {
            manufacturer_id: None,
            parse_extended: true,
        }
    }
}

impl LoRaPayloadDecoder for CompactFrameDecoder {
    fn decode(&self, payload: &[u8], f_port: u8) -> Result<MeteringData, LoRaDecodeError> {
        // Compact frames typically use fPort 1-10 for different message types
        // fPort 0 is reserved for MAC commands in LoRaWAN
        if f_port == 0 {
            return Err(LoRaDecodeError::InvalidData {
                offset: 0,
                reason: "fPort 0 is reserved for MAC commands".to_string(),
            });
        }

        // Check minimum length for compact frame
        if payload.len() < 12 {
            return Err(LoRaDecodeError::InvalidLength {
                expected: 12,
                actual: payload.len(),
            });
        }

        // Try to parse as wM-Bus frame
        let wmbus_frame = match parse_wmbus_frame(payload) {
            Ok(frame) => frame,
            Err(_) => {
                // If not a valid wM-Bus frame, try alternative compact format
                return self.decode_simple_compact(payload, f_port);
            }
        };

        // TODO: Convert wM-Bus frame to MeteringData
        // For now, return a simple representation
        Ok(MeteringData {
            timestamp: SystemTime::now(),
            readings: vec![Reading {
                value: MBusRecordValue::String(hex::encode(&wmbus_frame.payload)),
                unit: "hex".to_string(),
                quantity: "Raw wM-Bus payload".to_string(),
                tariff: None,
                storage_number: None,
                description: Some(format!("Device {:08X}", wmbus_frame.device_address)),
            }],
            battery: None,
            status: DeviceStatus::default(),
            raw_payload: payload.to_vec(),
            decoder_type: self.decoder_type().to_string(),
        })
    }

    fn decoder_type(&self) -> &str {
        "EN13757-Compact"
    }

    fn clone_box(&self) -> Box<dyn LoRaPayloadDecoder> {
        Box::new(self.clone())
    }

    fn can_decode(&self, payload: &[u8], f_port: u8) -> bool {
        // Check for compact frame markers
        if payload.len() < 12 || f_port == 0 {
            return false;
        }

        // Check for typical compact frame structure
        // Byte 0: Length indicator (0x1C-0x3C typical)
        // Byte 1: C field (0x44 for SND-NR, 0x46 for SND-IR)
        if payload[0] >= 0x1C && payload[0] <= 0x3C
            && (payload[1] == 0x44 || payload[1] == 0x46) {
                return true;
            }

        // Try parsing as wM-Bus frame
        parse_wmbus_frame(payload).is_ok()
    }
}

impl CompactFrameDecoder {
    /// Decode a simple compact format (non-wM-Bus standard)
    fn decode_simple_compact(
        &self,
        payload: &[u8],
        f_port: u8,
    ) -> Result<MeteringData, LoRaDecodeError> {
        // Simple compact format:
        // [0-3]: Device ID (4 bytes)
        // [4-7]: Counter value (4 bytes, little-endian)
        // [8-9]: Status (2 bytes)
        // [10]: Battery (1 byte, percentage)
        // [11+]: Optional extended data

        if payload.len() < 11 {
            return Err(LoRaDecodeError::InvalidLength {
                expected: 11,
                actual: payload.len(),
            });
        }

        let device_id = u32::from_le_bytes([payload[0], payload[1], payload[2], payload[3]]);
        let counter = u32::from_le_bytes([payload[4], payload[5], payload[6], payload[7]]);
        let status_flags = u16::from_le_bytes([payload[8], payload[9]]);
        let battery_percent = payload[10];

        // Determine unit based on fPort (example mapping)
        let (unit, quantity) = match f_port {
            1 => ("m³", "Volume"),
            2 => ("kWh", "Energy"),
            3 => ("L", "Volume"),
            4 => ("MWh", "Energy"),
            _ => ("units", "Count"),
        };

        let mut readings = vec![Reading {
            value: MBusRecordValue::Numeric(counter as f64),
            unit: unit.to_string(),
            quantity: quantity.to_string(),
            tariff: None,
            storage_number: Some(0),
            description: Some(format!("Device {device_id:#010X}")),
        }];

        // Parse extended data if present
        if payload.len() > 11 && self.parse_extended {
            // Example: temperature reading at offset 11-12
            if payload.len() >= 13 {
                let temp = i16::from_le_bytes([payload[11], payload[12]]) as f64 / 10.0;
                readings.push(Reading {
                    value: MBusRecordValue::Numeric(temp),
                    unit: "°C".to_string(),
                    quantity: "Temperature".to_string(),
                    tariff: None,
                    storage_number: None,
                    description: Some("Ambient temperature".to_string()),
                });
            }
        }

        Ok(MeteringData {
            timestamp: SystemTime::now(),
            readings,
            battery: Some(BatteryStatus {
                voltage: None,
                percentage: Some(battery_percent),
                low_battery: battery_percent < 20,
            }),
            status: DeviceStatus {
                alarm: (status_flags & 0x01) != 0,
                tamper: (status_flags & 0x02) != 0,
                leak: (status_flags & 0x04) != 0,
                reverse_flow: (status_flags & 0x08) != 0,
                error_code: if (status_flags & 0xFF00) != 0 {
                    Some((status_flags >> 8) & 0xFF)
                } else {
                    None
                },
                flags: status_flags as u32,
            },
            raw_payload: payload.to_vec(),
            decoder_type: self.decoder_type().to_string(),
        })
    }

    // TODO: Implement proper WMBusFrame to MeteringData conversion
    // The WMBusFrame structure doesn't have data_records, device_status, battery_status, or timestamp fields
    // This would need to parse the payload data to extract records
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_simple_compact_decode() {
        let decoder = CompactFrameDecoder::default();

        // Example payload: Device ID=0x12345678, Counter=1000, Status=0x0001, Battery=85%
        let payload = vec![
            0x78, 0x56, 0x34, 0x12, // Device ID (little-endian)
            0xE8, 0x03, 0x00, 0x00, // Counter = 1000 (little-endian)
            0x01, 0x00, // Status flags
            85,   // Battery percentage
            0x00, // Padding to meet 12-byte minimum
        ];

        let result = decoder.decode(&payload, 1).unwrap();

        assert_eq!(result.readings.len(), 1);
        match &result.readings[0].value {
            MBusRecordValue::Numeric(val) => assert_eq!(*val, 1000.0),
            _ => panic!("Expected numeric value"),
        }
        assert_eq!(result.readings[0].unit, "m³");

        assert_eq!(result.battery.as_ref().unwrap().percentage, Some(85));
        assert!(result.status.alarm);
        assert!(!result.status.tamper);
    }

    #[test]
    fn test_compact_with_temperature() {
        let decoder = CompactFrameDecoder::default();

        // Payload with temperature extension
        let payload = vec![
            0x78, 0x56, 0x34, 0x12, // Device ID
            0xE8, 0x03, 0x00, 0x00, // Counter = 1000
            0x00, 0x00, // Status flags
            85,   // Battery
            0x10, 0x01, // Temperature = 27.2°C (272 / 10)
        ];

        let result = decoder.decode(&payload, 1).unwrap();

        assert_eq!(result.readings.len(), 2);
        assert_eq!(result.readings[1].unit, "°C");
        match &result.readings[1].value {
            MBusRecordValue::Numeric(val) => assert_eq!(*val, 27.2),
            _ => panic!("Expected numeric temperature"),
        }
    }
}
