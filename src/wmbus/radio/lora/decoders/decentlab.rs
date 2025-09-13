//! Decentlab sensor decoder for LoRa payloads
//!
//! Handles Decentlab's standardized sensor format used across their product line.

use crate::payload::record::MBusRecordValue;
use crate::wmbus::radio::lora::decoder::{
    helpers, BatteryStatus, DecentlabChannel, DecentlabConfig, DeviceStatus, LoRaDecodeError,
    LoRaPayloadDecoder, MeteringData, Reading,
};
use std::time::SystemTime;

/// Decentlab sensor decoder
#[derive(Debug, Clone)]
pub struct DecentlabDecoder {
    pub config: DecentlabConfig,
}

impl DecentlabDecoder {
    pub fn new(config: DecentlabConfig) -> Self {
        Self { config }
    }

    /// Create decoder for DL-PR26 Pressure/Temperature sensor
    pub fn dl_pr26() -> Self {
        Self {
            config: DecentlabConfig {
                protocol_version: 2,
                channels: vec![
                    DecentlabChannel {
                        name: "Pressure".to_string(),
                        unit: "bar".to_string(),
                        scale_factor: 0.001,
                        offset: 0.0,
                    },
                    DecentlabChannel {
                        name: "Temperature".to_string(),
                        unit: "°C".to_string(),
                        scale_factor: 0.01,
                        offset: -273.15,
                    },
                ],
            },
        }
    }

    /// Create decoder for DL-TRS12 Temperature sensor
    pub fn dl_trs12() -> Self {
        Self {
            config: DecentlabConfig {
                protocol_version: 2,
                channels: vec![DecentlabChannel {
                    name: "Temperature".to_string(),
                    unit: "°C".to_string(),
                    scale_factor: 0.01,
                    offset: -273.15,
                }],
            },
        }
    }

    /// Create decoder for DL-PAR Photosynthetically Active Radiation sensor
    pub fn dl_par() -> Self {
        Self {
            config: DecentlabConfig {
                protocol_version: 2,
                channels: vec![DecentlabChannel {
                    name: "PAR".to_string(),
                    unit: "μmol⋅m⁻²⋅s⁻¹".to_string(),
                    scale_factor: 1.0,
                    offset: 0.0,
                }],
            },
        }
    }
}

impl LoRaPayloadDecoder for DecentlabDecoder {
    fn decode(&self, payload: &[u8], _f_port: u8) -> Result<MeteringData, LoRaDecodeError> {
        // Decentlab format:
        // [0]: Protocol version
        // [1-2]: Device ID (16-bit BE)
        // [3]: Sensor flags (bit mask indicating which sensors have data)
        // [4+]: Sensor data (16-bit BE values)
        // [last 2]: Battery voltage (16-bit BE, in mV)

        if payload.len() < 6 {
            return Err(LoRaDecodeError::InvalidLength {
                expected: 6,
                actual: payload.len(),
            });
        }

        let protocol_version = payload[0];
        if protocol_version != self.config.protocol_version {
            return Err(LoRaDecodeError::UnsupportedVersion(protocol_version));
        }

        let device_id = helpers::read_be_uint(payload, 1, 2)? as u16;
        let sensor_flags = payload[3];

        let mut offset = 4;
        let mut readings = Vec::new();

        // Parse sensor data based on flags
        for (channel_idx, channel) in self.config.channels.iter().enumerate() {
            // Check if this sensor has data (bit set in flags)
            if (sensor_flags & (1 << channel_idx)) != 0 {
                if payload.len() < offset + 2 {
                    return Err(LoRaDecodeError::InvalidLength {
                        expected: offset + 2,
                        actual: payload.len(),
                    });
                }

                let raw_value = helpers::read_be_uint(payload, offset, 2)? as i16;
                offset += 2;

                // Apply scaling and offset
                let value = (raw_value as f64 * channel.scale_factor) + channel.offset;

                readings.push(Reading {
                    value: MBusRecordValue::Numeric(value),
                    unit: channel.unit.clone(),
                    quantity: channel.name.clone(),
                    tariff: None,
                    storage_number: Some(channel_idx as u32),
                    description: Some(format!("Channel {}: {}", channel_idx, channel.name)),
                });
            }
        }

        // Parse battery voltage (last 2 bytes)
        let battery = if payload.len() >= offset + 2 {
            let battery_mv = helpers::read_be_uint(payload, offset, 2)? as u16;
            let voltage = battery_mv as f32 / 1000.0;
            let percentage = helpers::voltage_to_percentage(voltage, 2.0, 3.6);

            Some(BatteryStatus {
                voltage: Some(voltage),
                percentage: Some(percentage),
                low_battery: voltage < 2.2,
            })
        } else {
            None
        };

        Ok(MeteringData {
            timestamp: SystemTime::now(),
            readings,
            battery,
            status: DeviceStatus {
                flags: device_id as u32,
                ..Default::default()
            },
            raw_payload: payload.to_vec(),
            decoder_type: format!("Decentlab-{device_id:04X}"),
        })
    }

    fn decoder_type(&self) -> &str {
        "Decentlab"
    }

    fn clone_box(&self) -> Box<dyn LoRaPayloadDecoder> {
        Box::new(self.clone())
    }

    fn can_decode(&self, payload: &[u8], _f_port: u8) -> bool {
        // Check for Decentlab signature
        if payload.len() < 4 {
            return false;
        }

        // Protocol version check
        payload[0] == self.config.protocol_version
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_dl_pr26_decode() {
        let decoder = DecentlabDecoder::dl_pr26();

        // Example payload: Protocol=2, DeviceID=0x1234, Flags=0x03 (both sensors),
        // Pressure=1013 mbar, Temp=2315 (23.15°C + 273.15 offset), Battery=3300mV
        let payload = vec![
            0x02, // Protocol version
            0x12, 0x34, // Device ID
            0x03, // Sensor flags (both channels)
            0x03, 0xF5, // Pressure = 1013 mbar
            0x09, 0x0B, // Temperature raw = 2315
            0x0C, 0xE4, // Battery = 3300 mV
        ];

        let result = decoder.decode(&payload, 1).unwrap();

        assert_eq!(result.readings.len(), 2);

        // Check pressure
        match &result.readings[0].value {
            MBusRecordValue::Numeric(val) => assert!((val - 1.013).abs() < 0.001),
            _ => panic!("Expected numeric pressure"),
        }
        assert_eq!(result.readings[0].unit, "bar");

        // Check temperature (2315 * 0.01 - 273.15 = -250.0)
        match &result.readings[1].value {
            MBusRecordValue::Numeric(val) => assert!((val - (-250.0)).abs() < 0.01),
            _ => panic!("Expected numeric temperature"),
        }

        // Check battery
        assert_eq!(result.battery.as_ref().unwrap().voltage, Some(3.3));
    }

    #[test]
    fn test_partial_sensor_data() {
        let decoder = DecentlabDecoder::dl_pr26();

        // Only temperature sensor has data (flag = 0x02)
        let payload = vec![
            0x02, // Protocol version
            0xAB, 0xCD, // Device ID
            0x02, // Sensor flags (only temperature)
            0x09, 0x0B, // Temperature raw = 2315
            0x0B, 0xB8, // Battery = 3000 mV
        ];

        let result = decoder.decode(&payload, 1).unwrap();

        assert_eq!(result.readings.len(), 1);
        assert_eq!(result.readings[0].quantity, "Temperature");
    }
}
