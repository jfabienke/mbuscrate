//! Dragino sensor decoder for LoRa payloads
//!
//! Handles various Dragino water and environmental sensor formats.

use crate::payload::record::MBusRecordValue;
use crate::wmbus::radio::lora::decoder::{
    helpers, BatteryStatus, DeviceStatus, DraginoField, DraginoFormat, DraginoModel,
    LoRaDecodeError, LoRaPayloadDecoder, MeteringData, Reading,
};
use std::time::SystemTime;

/// Dragino sensor decoder
#[derive(Debug, Clone)]
pub struct DraginoDecoder {
    pub model: DraginoModel,
}

impl DraginoDecoder {
    pub fn new(model: DraginoModel) -> Self {
        Self { model }
    }

    /// Get the format specification for a model
    fn get_format(&self) -> DraginoFormat {
        match &self.model {
            DraginoModel::SW3L => DraginoFormat {
                name: "SW3L Water Flow Sensor".to_string(),
                fields: vec![
                    DraginoField {
                        name: "DeviceID".to_string(),
                        offset: 0,
                        size: 2,
                        unit: None,
                        scale: 1.0,
                    },
                    DraginoField {
                        name: "Status".to_string(),
                        offset: 2,
                        size: 1,
                        unit: None,
                        scale: 1.0,
                    },
                    DraginoField {
                        name: "FlowRate".to_string(),
                        offset: 3,
                        size: 2,
                        unit: Some("L/h".to_string()),
                        scale: 0.1,
                    },
                    DraginoField {
                        name: "TotalVolume".to_string(),
                        offset: 5,
                        size: 4,
                        unit: Some("L".to_string()),
                        scale: 0.001,
                    },
                    DraginoField {
                        name: "Temperature".to_string(),
                        offset: 9,
                        size: 2,
                        unit: Some("°C".to_string()),
                        scale: 0.01,
                    },
                    DraginoField {
                        name: "Battery".to_string(),
                        offset: 11,
                        size: 2,
                        unit: Some("mV".to_string()),
                        scale: 1.0,
                    },
                ],
            },
            DraginoModel::LWL03A => DraginoFormat {
                name: "LWL03A Water Leak Sensor".to_string(),
                fields: vec![
                    DraginoField {
                        name: "DeviceID".to_string(),
                        offset: 0,
                        size: 2,
                        unit: None,
                        scale: 1.0,
                    },
                    DraginoField {
                        name: "LeakStatus".to_string(),
                        offset: 2,
                        size: 1,
                        unit: None,
                        scale: 1.0,
                    },
                    DraginoField {
                        name: "LeakTimes".to_string(),
                        offset: 3,
                        size: 2,
                        unit: None,
                        scale: 1.0,
                    },
                    DraginoField {
                        name: "LeakDuration".to_string(),
                        offset: 5,
                        size: 2,
                        unit: Some("min".to_string()),
                        scale: 1.0,
                    },
                    DraginoField {
                        name: "Battery".to_string(),
                        offset: 7,
                        size: 2,
                        unit: Some("mV".to_string()),
                        scale: 1.0,
                    },
                ],
            },
            DraginoModel::Custom(format) => format.clone(),
        }
    }
}

impl LoRaPayloadDecoder for DraginoDecoder {
    fn decode(&self, payload: &[u8], _f_port: u8) -> Result<MeteringData, LoRaDecodeError> {
        let format = self.get_format();
        let mut readings = Vec::new();
        let mut battery = None;
        let mut status = DeviceStatus::default();
        let mut _device_id = 0u16;

        for field in &format.fields {
            // Check if field is within payload bounds
            if payload.len() < field.offset + field.size {
                continue; // Skip fields that don't fit in payload
            }

            // Read field value (little-endian by default for Dragino)
            let raw_value = helpers::read_le_uint(payload, field.offset, field.size)?;

            // Handle special fields
            match field.name.as_str() {
                "DeviceID" => {
                    _device_id = raw_value as u16;
                }
                "Status" | "LeakStatus" => {
                    let status_byte = raw_value as u8;
                    status.alarm = (status_byte & 0x01) != 0;
                    status.leak = field.name == "LeakStatus" && status_byte > 0;
                    status.flags = status_byte as u32;
                }
                "Battery" => {
                    let voltage = (raw_value as f32 * field.scale as f32) / 1000.0; // Convert mV to V
                    let percentage = helpers::voltage_to_percentage(voltage, 2.4, 3.6);

                    battery = Some(BatteryStatus {
                        voltage: Some(voltage),
                        percentage: Some(percentage),
                        low_battery: voltage < 2.5,
                    });
                }
                _ => {
                    // Regular measurement field
                    if let Some(unit) = &field.unit {
                        let value = raw_value as f64 * field.scale;

                        // Determine quantity from field name
                        let quantity = match field.name.as_str() {
                            "FlowRate" => "Flow Rate",
                            "TotalVolume" => "Volume",
                            "Temperature" => "Temperature",
                            "LeakTimes" => "Event Count",
                            "LeakDuration" => "Duration",
                            _ => "Measurement",
                        };

                        readings.push(Reading {
                            value: MBusRecordValue::Numeric(value),
                            unit: unit.clone(),
                            quantity: quantity.to_string(),
                            tariff: None,
                            storage_number: None,
                            description: Some(field.name.clone()),
                        });
                    }
                }
            }
        }

        Ok(MeteringData {
            timestamp: SystemTime::now(),
            readings,
            battery,
            status,
            raw_payload: payload.to_vec(),
            decoder_type: format!("Dragino-{}", format.name),
        })
    }

    fn decoder_type(&self) -> &str {
        "Dragino"
    }

    fn clone_box(&self) -> Box<dyn LoRaPayloadDecoder> {
        Box::new(self.clone())
    }

    fn can_decode(&self, payload: &[u8], _f_port: u8) -> bool {
        let format = self.get_format();

        // Check minimum required length
        let min_length = format
            .fields
            .iter()
            .map(|f| f.offset + f.size)
            .max()
            .unwrap_or(0);

        payload.len() >= min_length
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sw3l_decode() {
        let decoder = DraginoDecoder::new(DraginoModel::SW3L);

        // Example SW3L payload
        let payload = vec![
            0x12, 0x34, // Device ID = 0x3412
            0x00, // Status = OK
            0xE8, 0x03, // Flow rate = 1000 * 0.1 = 100 L/h
            0x10, 0x27, 0x00, 0x00, // Total = 10000 * 0.001 = 10 L
            0x10, 0x09, // Temperature = 2320 * 0.01 = 23.20°C
            0xE4, 0x0C, // Battery = 3300 mV
        ];

        let result = decoder.decode(&payload, 1).unwrap();

        // Should have flow rate, total volume, and temperature readings
        assert_eq!(result.readings.len(), 3);

        // Check flow rate
        let flow = result
            .readings
            .iter()
            .find(|r| r.description == Some("FlowRate".to_string()))
            .unwrap();
        match &flow.value {
            MBusRecordValue::Numeric(val) => assert_eq!(*val, 100.0),
            _ => panic!("Expected numeric flow rate"),
        }
        assert_eq!(flow.unit, "L/h");

        // Check total volume
        let volume = result
            .readings
            .iter()
            .find(|r| r.description == Some("TotalVolume".to_string()))
            .unwrap();
        match &volume.value {
            MBusRecordValue::Numeric(val) => assert_eq!(*val, 10.0),
            _ => panic!("Expected numeric volume"),
        }
        assert_eq!(volume.unit, "L");

        // Check temperature
        let temp = result
            .readings
            .iter()
            .find(|r| r.description == Some("Temperature".to_string()))
            .unwrap();
        match &temp.value {
            MBusRecordValue::Numeric(val) => assert_eq!(*val, 23.20),
            _ => panic!("Expected numeric temperature"),
        }

        // Check battery
        assert_eq!(result.battery.as_ref().unwrap().voltage, Some(3.3));
    }

    #[test]
    fn test_lwl03a_leak_detected() {
        let decoder = DraginoDecoder::new(DraginoModel::LWL03A);

        let payload = vec![
            0xAB, 0xCD, // Device ID
            0x01, // Leak detected
            0x05, 0x00, // 5 leak events
            0x1E, 0x00, // 30 minutes total
            0xB8, 0x0B, // Battery = 3000 mV
        ];

        let result = decoder.decode(&payload, 1).unwrap();

        // Check leak status
        assert!(result.status.leak);
        assert!(result.status.alarm);

        // Check leak count
        let times = result
            .readings
            .iter()
            .find(|r| r.description == Some("LeakTimes".to_string()))
            .unwrap();
        match &times.value {
            MBusRecordValue::Numeric(val) => assert_eq!(*val, 5.0),
            _ => panic!("Expected numeric leak count"),
        }

        // Check duration
        let duration = result
            .readings
            .iter()
            .find(|r| r.description == Some("LeakDuration".to_string()))
            .unwrap();
        match &duration.value {
            MBusRecordValue::Numeric(val) => assert_eq!(*val, 30.0),
            _ => panic!("Expected numeric duration"),
        }
        assert_eq!(duration.unit, "min");
    }
}
