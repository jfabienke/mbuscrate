//! Generic counter/pulse decoder for simple LoRa metering devices
//!
//! Handles common counter-based formats used by retrofit sensors and simple meters.

use crate::payload::record::MBusRecordValue;
use crate::wmbus::radio::lora::decoder::{
    helpers, BatteryStatus, DeviceStatus, GenericCounterConfig, LoRaDecodeError,
    LoRaPayloadDecoder, MeteringData, Reading,
};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

/// Generic counter decoder for simple pulse/counter formats
#[derive(Debug, Clone)]
pub struct GenericCounterDecoder {
    pub config: GenericCounterConfig,
}

impl GenericCounterDecoder {
    pub fn new(config: GenericCounterConfig) -> Self {
        Self { config }
    }

    /// Create a decoder for water meters (pulses to liters)
    pub fn water_meter(pulses_per_liter: f64) -> Self {
        Self {
            config: GenericCounterConfig {
                unit: "L".to_string(),
                scale_factor: 1.0 / pulses_per_liter,
                ..Default::default()
            },
        }
    }

    /// Create a decoder for gas meters (pulses to m³)
    pub fn gas_meter(pulses_per_m3: f64) -> Self {
        Self {
            config: GenericCounterConfig {
                unit: "m³".to_string(),
                scale_factor: 1.0 / pulses_per_m3,
                ..Default::default()
            },
        }
    }

    /// Create a decoder for electricity meters (pulses to kWh)
    pub fn electricity_meter(pulses_per_kwh: f64) -> Self {
        Self {
            config: GenericCounterConfig {
                unit: "kWh".to_string(),
                scale_factor: 1.0 / pulses_per_kwh,
                ..Default::default()
            },
        }
    }
}

impl LoRaPayloadDecoder for GenericCounterDecoder {
    fn decode(&self, payload: &[u8], _f_port: u8) -> Result<MeteringData, LoRaDecodeError> {
        let mut offset = 0;
        let mut timestamp = SystemTime::now();
        let mut battery = None;
        let mut status = DeviceStatus::default();

        // Parse timestamp if present
        if self.config.has_timestamp {
            if payload.len() < offset + 4 {
                return Err(LoRaDecodeError::InvalidLength {
                    expected: offset + 4,
                    actual: payload.len(),
                });
            }

            let unix_timestamp = if self.config.big_endian {
                helpers::read_be_uint(payload, offset, 4)?
            } else {
                helpers::read_le_uint(payload, offset, 4)?
            };

            timestamp = UNIX_EPOCH + Duration::from_secs(unix_timestamp);
            offset += 4;
        }

        // Parse counter value
        if payload.len() < offset + self.config.counter_size {
            return Err(LoRaDecodeError::InvalidLength {
                expected: offset + self.config.counter_size,
                actual: payload.len(),
            });
        }

        let counter_raw = if self.config.big_endian {
            helpers::read_be_uint(payload, offset, self.config.counter_size)?
        } else {
            helpers::read_le_uint(payload, offset, self.config.counter_size)?
        };
        offset += self.config.counter_size;

        let counter_value = counter_raw as f64 * self.config.scale_factor;

        // Parse optional delta value (change since last reading)
        let mut readings = vec![Reading {
            value: MBusRecordValue::Numeric(counter_value),
            unit: self.config.unit.clone(),
            quantity: "Cumulative".to_string(),
            tariff: None,
            storage_number: Some(0),
            description: Some("Total consumption".to_string()),
        }];

        // Check if there's a delta value (2 bytes)
        if payload.len() >= offset + 2 {
            let delta_raw = if self.config.big_endian {
                helpers::read_be_uint(payload, offset, 2)?
            } else {
                helpers::read_le_uint(payload, offset, 2)?
            };
            offset += 2;

            let delta_value = delta_raw as f64 * self.config.scale_factor;
            readings.push(Reading {
                value: MBusRecordValue::Numeric(delta_value),
                unit: self.config.unit.clone(),
                quantity: "Delta".to_string(),
                tariff: None,
                storage_number: None,
                description: Some("Change since last reading".to_string()),
            });
        }

        // Parse status byte if present
        if payload.len() > offset {
            let status_byte = payload[offset];
            offset += 1;

            status.alarm = (status_byte & 0x01) != 0;
            status.tamper = (status_byte & 0x02) != 0;
            status.leak = (status_byte & 0x04) != 0;
            status.reverse_flow = (status_byte & 0x08) != 0;
            status.flags = status_byte as u32;
        }

        // Parse battery if configured and present
        if self.config.has_battery && payload.len() > offset {
            let battery_byte = payload[offset];

            // Interpret as percentage (0-100) or voltage based on value range
            if battery_byte <= 100 {
                battery = Some(BatteryStatus {
                    voltage: None,
                    percentage: Some(battery_byte),
                    low_battery: battery_byte < 20,
                });
            } else {
                // Interpret as ADC value (0-255) mapped to voltage
                let voltage = helpers::adc_to_voltage(battery_byte as u16, 3.6, 255);
                let percentage = helpers::voltage_to_percentage(voltage, 2.4, 3.6);

                battery = Some(BatteryStatus {
                    voltage: Some(voltage),
                    percentage: Some(percentage),
                    low_battery: voltage < 2.5,
                });
            }
        }

        Ok(MeteringData {
            timestamp,
            readings,
            battery,
            status,
            raw_payload: payload.to_vec(),
            decoder_type: self.decoder_type().to_string(),
        })
    }

    fn decoder_type(&self) -> &str {
        "GenericCounter"
    }

    fn clone_box(&self) -> Box<dyn LoRaPayloadDecoder> {
        Box::new(self.clone())
    }

    fn can_decode(&self, payload: &[u8], _f_port: u8) -> bool {
        // Check minimum required length
        let min_length = if self.config.has_timestamp { 4 } else { 0 } + self.config.counter_size;

        if payload.len() < min_length {
            return false;
        }

        // Additional validation could check for reasonable values
        true
    }
}

/// Specialized decoder for standard pulse counter format
#[derive(Debug, Clone, Default)]
pub struct StandardPulseDecoder;

impl StandardPulseDecoder {
    /// Standard format:
    /// [0-3]: Counter (32-bit LE)
    /// [4-5]: Delta (16-bit LE)
    /// [6]: Status
    /// [7]: Battery %
    pub fn new() -> Self {
        Self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_basic_counter() {
        let decoder = GenericCounterDecoder::new(GenericCounterConfig::default());

        // Counter = 1000 (little-endian), battery = 75%
        let payload = vec![
            0xE8, 0x03, 0x00, 0x00, // Counter = 1000
            0x0A, 0x00, // Delta = 10 (little-endian)
            0x00, // Status = 0
            75,   // Battery = 75%
        ];

        let result = decoder.decode(&payload, 1).unwrap();

        assert_eq!(result.readings.len(), 2);

        // Check counter value
        match &result.readings[0].value {
            MBusRecordValue::Numeric(val) => assert_eq!(*val, 1000.0),
            _ => panic!("Expected numeric value"),
        }

        // Check delta value
        match &result.readings[1].value {
            MBusRecordValue::Numeric(val) => assert_eq!(*val, 10.0),
            _ => panic!("Expected numeric delta"),
        }

        assert_eq!(result.battery.as_ref().unwrap().percentage, Some(75));
    }

    #[test]
    fn test_water_meter() {
        // 10 pulses per liter
        let decoder = GenericCounterDecoder::water_meter(10.0);

        // 10000 pulses = 1000 liters
        let payload = vec![
            0x10, 0x27, 0x00, 0x00, // Counter = 10000 pulses
            0x64, 0x00, // Delta = 100 pulses
            0x00, // Status
            80,   // Battery
        ];

        let result = decoder.decode(&payload, 1).unwrap();

        match &result.readings[0].value {
            MBusRecordValue::Numeric(val) => assert_eq!(*val, 1000.0), // 1000 liters
            _ => panic!("Expected numeric value"),
        }
        assert_eq!(result.readings[0].unit, "L");

        match &result.readings[1].value {
            MBusRecordValue::Numeric(val) => assert_eq!(*val, 10.0), // 10 liters delta
            _ => panic!("Expected numeric delta"),
        }
    }

    #[test]
    fn test_with_timestamp() {
        let mut config = GenericCounterConfig::default();
        config.has_timestamp = true;
        let decoder = GenericCounterDecoder::new(config);

        // Unix timestamp (2024-01-01 00:00:00 = 1704067200)
        let payload = vec![
            0x80, 0x00, 0x92, 0x65, // Timestamp (LE)
            0xE8, 0x03, 0x00, 0x00, // Counter = 1000
            0x0A, 0x00, // Delta = 10 (little-endian)
            0x00, // Status
            90,   // Battery
        ];

        let result = decoder.decode(&payload, 1).unwrap();

        // Check timestamp is parsed correctly
        let duration = result.timestamp.duration_since(UNIX_EPOCH).unwrap();
        assert_eq!(duration.as_secs(), 1704067200);
    }

    #[test]
    fn test_status_flags() {
        let decoder = GenericCounterDecoder::new(GenericCounterConfig::default());

        let payload = vec![
            0x00, 0x00, 0x00, 0x00, // Counter = 0
            0x00, 0x00, // Delta = 0
            0x0F, // Status = all flags set
            100,  // Battery
        ];

        let result = decoder.decode(&payload, 1).unwrap();

        assert!(result.status.alarm);
        assert!(result.status.tamper);
        assert!(result.status.leak);
        assert!(result.status.reverse_flow);
    }
}
