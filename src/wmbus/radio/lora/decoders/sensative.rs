//! Sensative Strips decoder for LoRa payloads
//!
//! Handles Sensative's multi-sensor strips with TLV encoding.

use crate::payload::record::MBusRecordValue;
use crate::wmbus::radio::lora::decoder::{
    DeviceStatus, LoRaDecodeError, LoRaPayloadDecoder, MeteringData, Reading,
};
use std::time::SystemTime;

/// Sensative Strips decoder
#[derive(Debug, Clone)]
pub struct SensativeDecoder;

impl Default for SensativeDecoder {
    fn default() -> Self {
        Self::new()
    }
}

impl SensativeDecoder {
    pub fn new() -> Self {
        Self
    }
}

impl LoRaPayloadDecoder for SensativeDecoder {
    fn decode(&self, payload: &[u8], _f_port: u8) -> Result<MeteringData, LoRaDecodeError> {
        // Sensative uses TLV (Type-Length-Value) encoding
        // Type codes:
        // 0x01: Temperature
        // 0x02: Humidity
        // 0x03: Light
        // 0x04: Door/Window
        // 0x05: Presence
        // etc.

        let mut readings = Vec::new();
        let mut offset = 0;

        while offset < payload.len() {
            if offset + 2 > payload.len() {
                break;
            }

            let typ = payload[offset];
            let len = payload[offset + 1] as usize;
            offset += 2;

            if offset + len > payload.len() {
                break;
            }

            let data = &payload[offset..offset + len];
            offset += len;

            // Parse based on type
            match typ {
                0x01 => {
                    // Temperature (2 bytes, 0.01°C resolution)
                    if len == 2 {
                        let temp = i16::from_le_bytes([data[0], data[1]]) as f64 / 100.0;
                        readings.push(Reading {
                            value: MBusRecordValue::Numeric(temp),
                            unit: "°C".to_string(),
                            quantity: "Temperature".to_string(),
                            tariff: None,
                            storage_number: None,
                            description: Some("Temperature sensor".to_string()),
                        });
                    }
                }
                0x02 => {
                    // Humidity (1 byte, 0.5% resolution)
                    if len == 1 {
                        let humidity = data[0] as f64 * 0.5;
                        readings.push(Reading {
                            value: MBusRecordValue::Numeric(humidity),
                            unit: "%".to_string(),
                            quantity: "Humidity".to_string(),
                            tariff: None,
                            storage_number: None,
                            description: Some("Humidity sensor".to_string()),
                        });
                    }
                }
                _ => {
                    // Unknown type - skip
                }
            }
        }

        Ok(MeteringData {
            timestamp: SystemTime::now(),
            readings,
            battery: None,
            status: DeviceStatus::default(),
            raw_payload: payload.to_vec(),
            decoder_type: "Sensative".to_string(),
        })
    }

    fn decoder_type(&self) -> &str {
        "Sensative"
    }

    fn clone_box(&self) -> Box<dyn LoRaPayloadDecoder> {
        Box::new(self.clone())
    }
}
